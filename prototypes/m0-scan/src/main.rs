//! Milestone 0 feasibility prototype: how fast can we build a line index for
//! large files, and what does each strategy cost in memory?
//!
//! Usage:
//!   m0-scan gen <path> <size_mb> [long]   generate synthetic file (long = single line)
//!   m0-scan pread <path>                  bounded-chunk read + memchr newline count
//!   m0-scan mmap <path>                   memory-mapped + memchr newline count
//!   m0-scan index <path>                  build sparse line index (every 1024th line)
//!   m0-scan nobuf <path>                  like index, but bypassing the OS cache (Windows; = cold read)
//!   m0-scan ropey <path>                  load whole file into a ropey::Rope
//!
//! Prints elapsed time and peak resident memory (Linux VmHWM, Windows PeakWorkingSetSize).

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::time::Instant;

const CHUNK: usize = 1 << 20; // 1 MiB

/// Peak resident memory in KiB: Linux `VmHWM`, Windows `PeakWorkingSetSize`.
#[cfg(not(windows))]
fn peak_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

#[cfg(windows)]
#[allow(clippy::undocumented_unsafe_blocks)]
fn peak_rss_kb() -> u64 {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut pmc = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    match unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) } {
        Ok(()) => pmc.PeakWorkingSetSize as u64 / 1024,
        Err(_) => 0,
    }
}

/// Unbuffered (cache-bypassing) read + sparse index: equivalent to a cold read.
/// `FILE_FLAG_NO_BUFFERING` needs sector-aligned buffers and transfer sizes,
/// obtained here without `unsafe` by aligning inside an oversized `Vec`.
#[cfg(windows)]
fn nobuf_index(path: &str) -> std::io::Result<(u64, usize)> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_NO_BUFFERING, FILE_FLAG_SEQUENTIAL_SCAN};
    const ALIGN: usize = 4096;
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags((FILE_FLAG_NO_BUFFERING | FILE_FLAG_SEQUENTIAL_SCAN).0)
        .open(path)?;
    let mut storage = vec![0u8; CHUNK + ALIGN];
    let offset = storage.as_ptr().align_offset(ALIGN);
    let buf = &mut storage[offset..offset + CHUNK];
    let mut index: Vec<u64> = vec![0];
    let (mut lines, mut base) = (0u64, 0u64);
    loop {
        let n = f.read(buf)?;
        if n == 0 {
            break;
        }
        for pos in memchr::memchr_iter(b'\n', &buf[..n]) {
            lines += 1;
            if lines.is_multiple_of(1024) {
                index.push(base + pos as u64 + 1);
            }
        }
        base += n as u64;
    }
    Ok((lines, index.len()))
}

#[cfg(not(windows))]
fn nobuf_index(_path: &str) -> std::io::Result<(u64, usize)> {
    Err(std::io::Error::other("nobuf mode is Windows-only"))
}

fn generate(path: &str, size_mb: u64, long: bool) -> std::io::Result<()> {
    let mut w = BufWriter::with_capacity(CHUNK, File::create(path)?);
    let target = size_mb * 1024 * 1024;
    let mut written = 0u64;
    let mut n = 0u64;
    while written < target {
        let line = if long {
            format!("{n:010} lorem ipsum dolor sit amet, ünïcödé ✓ 漢字 ")
        } else {
            format!(
                "2026-11-20T10:{:02}:{:02}Z INFO request id={n} path=/api/v1/items ünïcödé ✓\r\n",
                (n / 60) % 60,
                n % 60
            )
        };
        w.write_all(line.as_bytes())?;
        written += line.len() as u64;
        n += 1;
    }
    w.flush()
}

fn pread_count(path: &str) -> std::io::Result<u64> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; CHUNK];
    let mut lines = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        lines += memchr::memchr_iter(b'\n', &buf[..n]).count() as u64;
    }
    Ok(lines)
}

fn mmap_count(path: &str) -> std::io::Result<u64> {
    let f = File::open(path)?;
    // SAFETY: prototype only; the file is not modified while mapped.
    let map = unsafe { memmap2::Mmap::map(&f)? };
    Ok(memchr::memchr_iter(b'\n', &map).count() as u64)
}

/// Sparse index: byte offset of every 1024th line start. Memory is
/// O(lines / 1024) * 8 bytes, i.e. ~8 KB per million lines.
fn sparse_index(path: &str) -> std::io::Result<(u64, usize)> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; CHUNK];
    let mut index: Vec<u64> = vec![0];
    let mut lines = 0u64;
    let mut base = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for pos in memchr::memchr_iter(b'\n', &buf[..n]) {
            lines += 1;
            if lines.is_multiple_of(1024) {
                index.push(base + pos as u64 + 1);
            }
        }
        base += n as u64;
    }
    Ok((lines, index.len()))
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, path) = (args.get(1).map(String::as_str), args.get(2));
    let Some(path) = path else {
        eprintln!("usage: m0-scan <gen|pread|mmap|index|nobuf|ropey> <path> [...]");
        std::process::exit(2);
    };
    let t = Instant::now();
    let result = match cmd {
        Some("gen") => {
            let mb = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100);
            generate(path, mb, args.get(4).is_some_and(|s| s == "long"))?;
            "generated".to_string()
        }
        Some("pread") => format!("lines={}", pread_count(path)?),
        Some("mmap") => format!("lines={}", mmap_count(path)?),
        Some("index") => {
            let (l, e) = sparse_index(path)?;
            format!("lines={l} index_entries={e}")
        }
        Some("nobuf") => {
            let (l, e) = nobuf_index(path)?;
            format!("lines={l} index_entries={e}")
        }
        Some("ropey") => {
            let rope = ropey::Rope::from_reader(File::open(path)?)?;
            format!("lines={} bytes={}", rope.len_lines(), rope.len_bytes())
        }
        _ => {
            eprintln!("unknown command");
            std::process::exit(2);
        }
    };
    let secs = t.elapsed().as_secs_f64();
    let size = std::fs::metadata(path)?.len() as f64 / (1024.0 * 1024.0);
    println!(
        "{:6} {result:40} {secs:8.3}s {:8.0} MiB/s peak_rss={} MiB",
        cmd.unwrap_or(""),
        size / secs,
        peak_rss_kb() / 1024
    );
    Ok(())
}
