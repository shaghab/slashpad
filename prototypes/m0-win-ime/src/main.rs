//! M0 spike: is IMM32 enough for inline IME composition in a custom-drawn editor? (risk R-02)
//!
//! ```text
//! m0-win-ime.exe [log-file]      default: m0-win-ime.log in the current directory
//! ```
//!
//! A window with a trivially simple custom-drawn text area (caret always at the
//! end). It draws the IME composition string inline (underlined) at the caret,
//! positions the IME candidate window under it, and logs every IME message with
//! its strings so the session can be reviewed afterwards. Manual test checklist:
//! see `prototypes/README.md`.
//!
//! Prototype code: `unsafe` is used freely and is NOT a model for the product.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("m0-win-ime only runs on Windows");
}

#[cfg(windows)]
fn main() {
    if let Err(e) = win::run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
#[allow(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]
mod win {
    use std::cell::RefCell;
    use std::fs::File;
    use std::io::Write;

    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::Input::Ime::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{Result, w};

    const MARGIN: i32 = 12;

    struct State {
        /// Committed text (UTF-16), caret always at the end.
        text: Vec<u16>,
        /// Current composition string and the IME's cursor position within it.
        comp: Vec<u16>,
        comp_cursor: usize,
        font: HFONT,
        underline_font: HFONT,
        line_height: i32,
        log: Option<File>,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
        STATE.with(|s| s.borrow_mut().as_mut().map(f))
    }

    fn log(line: &str) {
        with_state(|st| {
            if let Some(f) = st.log.as_mut() {
                let _ = writeln!(f, "{line}");
                let _ = f.flush();
            }
        });
    }

    fn describe(units: &[u16]) -> String {
        let text = String::from_utf16_lossy(units);
        let hex: Vec<String> = units.iter().map(|u| format!("{u:04X}")).collect();
        format!("{text:?} [{}]", hex.join(" "))
    }

    pub fn run() -> Result<()> {
        let log_path = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "m0-win-ime.log".into());
        let log_file = File::create(&log_path).ok();
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_IBEAM)?,
                hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut _),
                lpszClassName: w!("SlashPadImeSpike"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(windows::core::Error::from_thread());
            }
            STATE.with(|s| {
                *s.borrow_mut() = Some(State {
                    text: Vec::new(),
                    comp: Vec::new(),
                    comp_cursor: 0,
                    font: HFONT::default(),
                    underline_font: HFONT::default(),
                    line_height: 20,
                    log: log_file,
                })
            });
            log(&format!(
                "keyboard layout at start: {:?}",
                GetKeyboardLayout(0)
            ));
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("SlashPadImeSpike"),
                w!("SlashPad IME spike — type with Japanese / Chinese / Korean IME"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                900,
                500,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            create_fonts(hwnd);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    unsafe fn create_fonts(hwnd: HWND) {
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let height = -(20 * dpi / 96);
        let make = |underline: u32| {
            CreateFontW(
                height,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                underline,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Yu Gothic UI"),
            )
        };
        let (font, underline_font) = (make(0), make(1));
        with_state(|st| {
            st.font = font;
            st.underline_font = underline_font;
            st.line_height = (-height) * 3 / 2;
        });
    }

    /// Client coordinates of the caret: end of committed text plus the
    /// composition text up to the IME cursor.
    unsafe fn caret_point(hwnd: HWND) -> POINT {
        let (lines, comp_prefix, font, lh) = with_state(|st| {
            let lines: Vec<Vec<u16>> = st.text.split(|&u| u == 10).map(|l| l.to_vec()).collect();
            (
                lines,
                st.comp[..st.comp_cursor.min(st.comp.len())].to_vec(),
                st.font,
                st.line_height,
            )
        })
        .unwrap_or_default();
        let hdc = GetDC(Some(hwnd));
        let old = SelectObject(hdc, font.into());
        let last = lines.last().cloned().unwrap_or_default();
        let mut size = SIZE::default();
        let mut run = last;
        run.extend_from_slice(&comp_prefix);
        let _ = GetTextExtentPoint32W(hdc, &run, &mut size);
        SelectObject(hdc, old);
        ReleaseDC(Some(hwnd), hdc);
        POINT {
            x: MARGIN + size.cx,
            y: MARGIN + (lines.len().max(1) as i32 - 1) * lh,
        }
    }

    unsafe fn position_ime_windows(hwnd: HWND) {
        let pt = caret_point(hwnd);
        let lh = with_state(|st| st.line_height).unwrap_or(20);
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            log("ImmGetContext returned null (no IME active?)");
            return;
        }
        let comp = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: pt,
            rcArea: RECT::default(),
        };
        let _ = ImmSetCompositionWindow(himc, &comp);
        let cand = CANDIDATEFORM {
            dwIndex: 0,
            dwStyle: CFS_EXCLUDE,
            ptCurrentPos: pt,
            rcArea: RECT {
                left: pt.x,
                top: pt.y,
                right: pt.x + 1,
                bottom: pt.y + lh,
            },
        };
        let _ = ImmSetCandidateWindow(himc, &cand);
        let _ = ImmReleaseContext(hwnd, himc);
        let _ = SetCaretPos(pt.x, pt.y);
    }

    unsafe fn read_comp_string(himc: HIMC, kind: IME_COMPOSITION_STRING) -> Vec<u16> {
        let bytes = ImmGetCompositionStringW(himc, kind, None, 0);
        if bytes <= 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; bytes as usize / 2];
        ImmGetCompositionStringW(himc, kind, Some(buf.as_mut_ptr().cast()), bytes as u32);
        buf
    }

    extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_SETFOCUS => {
                    let lh = with_state(|st| st.line_height).unwrap_or(20);
                    let _ = CreateCaret(hwnd, None, 2, lh * 2 / 3);
                    position_ime_windows(hwnd);
                    let _ = ShowCaret(Some(hwnd));
                    LRESULT(0)
                }
                WM_KILLFOCUS => {
                    let _ = DestroyCaret();
                    LRESULT(0)
                }
                WM_IME_SETCONTEXT => {
                    log(&format!(
                        "WM_IME_SETCONTEXT active={} lparam={:#x}",
                        wparam.0, lparam.0
                    ));
                    // We draw the composition ourselves: hide the IME's own composition window.
                    let lp = LPARAM(lparam.0 & !(ISC_SHOWUICOMPOSITIONWINDOW as isize));
                    DefWindowProcW(hwnd, msg, wparam, lp)
                }
                WM_IME_STARTCOMPOSITION => {
                    log("WM_IME_STARTCOMPOSITION");
                    position_ime_windows(hwnd);
                    LRESULT(0) // not forwarded: no default composition window
                }
                WM_IME_COMPOSITION => {
                    let flags = lparam.0 as u32;
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        if flags & GCS_RESULTSTR.0 != 0 {
                            let result = read_comp_string(himc, GCS_RESULTSTR);
                            log(&format!("WM_IME_COMPOSITION RESULT {}", describe(&result)));
                            with_state(|st| st.text.extend_from_slice(&result));
                        }
                        if flags & GCS_COMPSTR.0 != 0 {
                            let comp = read_comp_string(himc, GCS_COMPSTR);
                            let cursor = ImmGetCompositionStringW(himc, GCS_CURSORPOS, None, 0)
                                .max(0) as usize;
                            log(&format!(
                                "WM_IME_COMPOSITION COMP {} cursor={cursor}",
                                describe(&comp)
                            ));
                            with_state(|st| {
                                st.comp = comp;
                                st.comp_cursor = cursor;
                            });
                        } else if flags & GCS_RESULTSTR.0 != 0 {
                            with_state(|st| st.comp.clear());
                        }
                        let _ = ImmReleaseContext(hwnd, himc);
                    }
                    position_ime_windows(hwnd);
                    let _ = InvalidateRect(Some(hwnd), None, true);
                    LRESULT(0) // handled: no WM_IME_CHAR/WM_CHAR duplicates
                }
                WM_IME_ENDCOMPOSITION => {
                    log("WM_IME_ENDCOMPOSITION");
                    with_state(|st| st.comp.clear());
                    let _ = InvalidateRect(Some(hwnd), None, true);
                    LRESULT(0)
                }
                WM_IME_NOTIFY => {
                    log(&format!("WM_IME_NOTIFY command={:#x}", wparam.0));
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
                WM_IME_REQUEST if wparam.0 as u32 == IMR_QUERYCHARPOSITION => {
                    // Modern (TSF-based) IMEs ask where a composition character is.
                    let pos = lparam.0 as *mut IMECHARPOSITION;
                    let mut pt = caret_point(hwnd);
                    let lh = with_state(|st| st.line_height).unwrap_or(20);
                    let _ = ClientToScreen(hwnd, &mut pt);
                    let mut doc = RECT::default();
                    let _ = GetClientRect(hwnd, &mut doc);
                    let mut tl = POINT {
                        x: doc.left,
                        y: doc.top,
                    };
                    let mut br = POINT {
                        x: doc.right,
                        y: doc.bottom,
                    };
                    let _ = ClientToScreen(hwnd, &mut tl);
                    let _ = ClientToScreen(hwnd, &mut br);
                    if !pos.is_null() {
                        log(&format!(
                            "WM_IME_REQUEST IMR_QUERYCHARPOSITION char={}",
                            (*pos).dwCharPos
                        ));
                        (*pos).pt = pt;
                        (*pos).cLineHeight = lh as u32;
                        (*pos).rcDocument = RECT {
                            left: tl.x,
                            top: tl.y,
                            right: br.x,
                            bottom: br.y,
                        };
                    }
                    LRESULT(1)
                }
                WM_IME_REQUEST => {
                    log(&format!(
                        "WM_IME_REQUEST {:#x} (default handling)",
                        wparam.0
                    ));
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
                WM_CHAR => {
                    let unit = wparam.0 as u16;
                    log(&format!("WM_CHAR {}", describe(&[unit])));
                    with_state(|st| match unit {
                        8 => {
                            // Backspace: remove one UTF-16 code point (surrogate-aware).
                            if let Some(last) = st.text.pop()
                                && (0xDC00..=0xDFFF).contains(&last)
                                && st
                                    .text
                                    .last()
                                    .is_some_and(|u| (0xD800..=0xDBFF).contains(u))
                            {
                                st.text.pop();
                            }
                        }
                        13 => st.text.push(10),
                        u if u >= 32 || u == 9 => st.text.push(u),
                        _ => {}
                    });
                    position_ime_windows(hwnd);
                    let _ = InvalidateRect(Some(hwnd), None, true);
                    LRESULT(0)
                }
                WM_DPICHANGED => {
                    create_fonts(hwnd);
                    let suggested = &*(lparam.0 as *const RECT);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    LRESULT(0)
                }
                WM_PAINT => {
                    paint(hwnd);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }

    unsafe fn paint(hwnd: HWND) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let snapshot = with_state(|st| {
            (
                st.text.clone(),
                st.comp.clone(),
                st.font,
                st.underline_font,
                st.line_height,
            )
        });
        if let Some((text, comp, font, underline_font, lh)) = snapshot {
            let old = SelectObject(hdc, font.into());
            let lines: Vec<&[u16]> = text.split(|&u| u == 10).collect();
            let mut y = MARGIN;
            for line in &lines {
                let _ = TextOutW(hdc, MARGIN, y, line);
                y += lh;
            }
            if !comp.is_empty() {
                // Draw the composition string, underlined, right after the last line.
                let mut size = SIZE::default();
                let last = lines.last().copied().unwrap_or(&[]);
                let _ = GetTextExtentPoint32W(hdc, last, &mut size);
                SelectObject(hdc, underline_font.into());
                let y_last = MARGIN + (lines.len().max(1) as i32 - 1) * lh;
                let _ = SetTextColor(hdc, COLORREF(0x00AA5500));
                let _ = TextOutW(hdc, MARGIN + size.cx, y_last, &comp);
            }
            SelectObject(hdc, old);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}
