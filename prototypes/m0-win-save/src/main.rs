//! M0 spike: does atomic save via `ReplaceFileW` behave as ADR-0006/0007 assume? (risk R-05)
//!
//! ```text
//! m0-win-save.exe [directory]     default: %TEMP%\slashpad-save-spike
//! ```
//! Run it once on a local NTFS drive, and if possible on an SMB share and a
//! FAT32/exFAT USB stick. Prints one line per scenario; exit code 1 if any
//! scenario left the ORIGINAL file damaged (an integrity failure).

#[cfg(not(windows))]
fn main() {
    eprintln!("m0-win-save only runs on Windows");
}

#[cfg(windows)]
fn main() {
    std::process::exit(win::run());
}

#[cfg(windows)]
#[allow(clippy::undocumented_unsafe_blocks)]
mod win {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use std::path::{Path, PathBuf};

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetFileInformationByHandle, REPLACE_FILE_FLAGS, ReplaceFileW,
    };
    use windows::core::HSTRING;

    const OLD: &[u8] = b"ORIGINAL CONTENT\r\nline 2\r\n";
    const NEW: &[u8] = b"NEW CONTENT\r\nsaved by SlashPad\r\n";

    struct FileInfo {
        id: u64,
        links: u32,
        created: u64,
        attributes: u32,
    }

    fn info(path: &Path) -> std::io::Result<FileInfo> {
        let f = File::open(path)?;
        let mut bhfi = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut bhfi) }
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let meta = f.metadata()?;
        Ok(FileInfo {
            id: (u64::from(bhfi.nFileIndexHigh) << 32) | u64::from(bhfi.nFileIndexLow),
            links: bhfi.nNumberOfLinks,
            created: meta.creation_time(),
            attributes: meta.file_attributes(),
        })
    }

    /// Writes `NEW` to a temp file next to `target`, flushes it, and replaces `target`.
    fn atomic_save(target: &Path) -> Result<(), String> {
        let tmp = target.with_extension("slashpad-tmp");
        let result = (|| {
            let mut f = File::create(&tmp).map_err(|e| format!("create temp: {e}"))?;
            f.write_all(NEW).map_err(|e| format!("write temp: {e}"))?;
            f.sync_all().map_err(|e| format!("flush temp: {e}"))?; // FlushFileBuffers
            drop(f);
            unsafe {
                ReplaceFileW(
                    &HSTRING::from(target.as_os_str()),
                    &HSTRING::from(tmp.as_os_str()),
                    None,
                    REPLACE_FILE_FLAGS(0),
                    None,
                    None,
                )
            }
            .map_err(|e| format!("ReplaceFileW: {e}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    fn fresh(dir: &Path, name: &str) -> std::io::Result<PathBuf> {
        let path = dir.join(name);
        if path.exists() {
            let mut perms = fs::metadata(&path)?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(&path, perms)?;
            fs::remove_file(&path)?;
        }
        fs::write(&path, OLD)?;
        Ok(path)
    }

    fn read_all(path: &Path) -> Vec<u8> {
        fs::read(path).unwrap_or_default()
    }

    type Check = fn(&Path, &mut Report) -> std::io::Result<()>;

    struct Report {
        failures_of_integrity: u32,
    }

    impl Report {
        fn line(&self, scenario: &str, outcome: &str, notes: &str) {
            println!("{scenario:<44} | {outcome:<46} | {notes}");
        }
    }

    pub fn run() -> i32 {
        let dir = std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("slashpad-save-spike"));
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("cannot create {}: {e}", dir.display());
            return 2;
        }
        println!("Directory: {}", dir.display());
        println!("{:<44} | {:<46} | notes", "scenario", "outcome");
        println!("{}", "-".repeat(120));
        let mut r = Report {
            failures_of_integrity: 0,
        };
        let checks: [(&str, Check); 6] = [
            ("S1 plain replace", s1_plain),
            ("S2 we hold a shared read handle", s2_our_handle),
            ("S3 other handle without SHARE_DELETE", s3_no_share_delete),
            ("S4 read-only original", s4_read_only),
            ("S5 original has a second hard link", s5_hard_link),
            ("S6 alternate data stream on original", s6_ads),
        ];
        for (name, check) in checks {
            if let Err(e) = check(&dir, &mut r) {
                r.line(name, "SETUP ERROR", &e.to_string());
            }
        }
        println!("{}", "-".repeat(120));
        if r.failures_of_integrity == 0 {
            println!("INTEGRITY: original was intact or correctly replaced in every scenario");
            0
        } else {
            println!(
                "INTEGRITY: {} scenario(s) damaged the original",
                r.failures_of_integrity
            );
            1
        }
    }

    /// After a save attempt, the file must hold exactly OLD (failed) or NEW (succeeded).
    fn verify(path: &Path, saved: &Result<(), String>, r: &mut Report) -> String {
        let content = read_all(path);
        let expected = if saved.is_ok() { NEW } else { OLD };
        let tmp_left = path.with_extension("slashpad-tmp").exists();
        if content != expected {
            r.failures_of_integrity += 1;
            return format!(
                "INTEGRITY FAILURE: content is {:?}",
                String::from_utf8_lossy(&content)
            );
        }
        if tmp_left {
            "content OK, temp file left behind".into()
        } else {
            "content OK".into()
        }
    }

    fn outcome(saved: &Result<(), String>) -> String {
        match saved {
            Ok(()) => "saved".into(),
            Err(e) => format!("refused: {e}"),
        }
    }

    fn s1_plain(dir: &Path, r: &mut Report) -> std::io::Result<()> {
        let path = fresh(dir, "s1.txt")?;
        let before = info(&path)?;
        let saved = atomic_save(&path);
        let v = verify(&path, &saved, r);
        let after = info(&path)?;
        r.line(
            "S1 plain replace",
            &outcome(&saved),
            &format!(
                "{v}; creation time {}; file id {}",
                if before.created == after.created {
                    "preserved"
                } else {
                    "CHANGED"
                },
                if before.id == after.id {
                    "same"
                } else {
                    "changed"
                },
            ),
        );
        Ok(())
    }

    fn s2_our_handle(dir: &Path, r: &mut Report) -> std::io::Result<()> {
        let path = fresh(dir, "s2.txt")?;
        let share = (FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0;
        let mut held = OpenOptions::new()
            .read(true)
            .share_mode(share)
            .open(&path)?;
        let saved = atomic_save(&path);
        let v = verify(&path, &saved, r);
        let mut old_view = Vec::new();
        held.seek(SeekFrom::Start(0))?;
        let read_back = held.read_to_end(&mut old_view);
        let handle_note = match read_back {
            Ok(_) if old_view == OLD => {
                "held handle still reads OLD content (as ADR-0006 assumes)".to_string()
            }
            Ok(_) => format!("held handle reads {:?}", String::from_utf8_lossy(&old_view)),
            Err(e) => format!("held handle read error: {e}"),
        };
        r.line(
            "S2 we hold a shared read handle",
            &outcome(&saved),
            &format!("{v}; {handle_note}"),
        );
        Ok(())
    }

    fn s3_no_share_delete(dir: &Path, r: &mut Report) -> std::io::Result<()> {
        let path = fresh(dir, "s3.txt")?;
        // Like a log writer or another editor that shares read/write but not delete.
        let share = (FILE_SHARE_READ | FILE_SHARE_WRITE).0;
        let _other = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(share)
            .open(&path)?;
        let saved = atomic_save(&path);
        let v = verify(&path, &saved, r);
        r.line("S3 other handle without SHARE_DELETE", &outcome(&saved), &v);
        Ok(())
    }

    fn s4_read_only(dir: &Path, r: &mut Report) -> std::io::Result<()> {
        let path = fresh(dir, "s4.txt")?;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms)?;
        let saved = atomic_save(&path);
        let v = verify(&path, &saved, r);
        let ro = info(&path)?.attributes & 0x1 != 0;
        r.line(
            "S4 read-only original",
            &outcome(&saved),
            &format!(
                "{v}; read-only attribute {}",
                if ro { "kept" } else { "LOST" }
            ),
        );
        Ok(())
    }

    fn s5_hard_link(dir: &Path, r: &mut Report) -> std::io::Result<()> {
        let path = fresh(dir, "s5.txt")?;
        let link = dir.join("s5-link.txt");
        let _ = fs::remove_file(&link);
        if let Err(e) = fs::hard_link(&path, &link) {
            r.line(
                "S5 original has a second hard link",
                "SKIPPED",
                &format!("hard links unsupported: {e}"),
            );
            return Ok(());
        }
        let links = info(&path)?.links;
        let saved = atomic_save(&path);
        let v = verify(&path, &saved, r);
        let link_content = read_all(&link);
        let link_note = if link_content == OLD {
            "other link still has OLD content (link broken)"
        } else {
            "other link has NEW content"
        };
        r.line(
            "S5 original has a second hard link",
            &outcome(&saved),
            &format!("{v}; links before: {links}; {link_note}"),
        );
        Ok(())
    }

    fn s6_ads(dir: &Path, r: &mut Report) -> std::io::Result<()> {
        let path = fresh(dir, "s6.txt")?;
        let stream = PathBuf::from(format!("{}:slashpad.test", path.display()));
        if let Err(e) = fs::write(&stream, b"stream data") {
            r.line(
                "S6 alternate data stream on original",
                "SKIPPED",
                &format!("no ADS support: {e}"),
            );
            return Ok(());
        }
        let saved = atomic_save(&path);
        let v = verify(&path, &saved, r);
        let ads = match fs::read(&stream) {
            Ok(d) if d == b"stream data" => "stream preserved",
            Ok(_) => "stream changed",
            Err(_) => "stream LOST",
        };
        r.line(
            "S6 alternate data stream on original",
            &outcome(&saved),
            &format!("{v}; {ads}"),
        );
        Ok(())
    }
}
