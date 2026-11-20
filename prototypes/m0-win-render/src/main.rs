//! M0 spike: startup time and memory cost of candidate renderers (risk R-01).
//!
//! Opens one window, draws a few lines of mixed-script text with the chosen
//! renderer, waits until idle, then writes a JSON line of measurements and exits.
//!
//! ```text
//! m0-win-render.exe <mode> <out.jsonl> [idle_ms]
//!   mode: empty | gdi | d2d | dwrite-gdi
//! ```
//!
//! Measurements (all from inside the process):
//! - `first_paint_ms`: process creation → first paint finished (includes loader time).
//! - `private_ws_kb`, `private_commit_kb`, `ws_kb`, `peak_ws_kb`: after `idle_ms`.
//!
//! Prototype code: `unsafe` is used freely here and is NOT a model for the product.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("m0-win-render only runs on Windows");
}

#[cfg(windows)]
fn main() {
    if let Err(e) = win::run() {
        win::fail(&format!("{e}"));
    }
}

#[cfg(windows)]
#[allow(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]
mod win {
    use std::cell::RefCell;
    use std::io::Write;

    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Direct2D::Common::*;
    use windows::Win32::Graphics::Direct2D::*;
    use windows::Win32::Graphics::DirectWrite::*;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{BOOL, IUnknown, PCWSTR, Ref, Result, implement, w};

    const SAMPLE: &str = "SlashPad — Less memory. More text.\n\
        The quick brown fox jumps over the lazy dog. 0123456789\n\
        日本語のテキスト · 中文文本 · 한국어 텍스트\n\
        العربية · עברית · ελληνικά · кириллица\n\
        Emoji: 😀 👍🏽 👨‍👩‍👧";
    const FONT: PCWSTR = w!("Segoe UI");
    const FONT_PX: f32 = 20.0;
    const IDLE_TIMER: usize = 1;

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Empty,
        Gdi,
        D2d,
        DwriteGdi,
    }

    struct State {
        mode: Mode,
        out: String,
        idle_ms: u32,
        t_created: u64,
        first_paint_ms: Option<f64>,
        d2d: Option<D2d>,
        dw: Option<DwGdi>,
    }

    struct D2d {
        factory: ID2D1Factory,
        dwrite: IDWriteFactory,
        format: IDWriteTextFormat,
        target: Option<ID2D1HwndRenderTarget>,
    }

    struct DwGdi {
        dwrite: IDWriteFactory,
        format: IDWriteTextFormat,
        interop: IDWriteGdiInterop,
        params: IDWriteRenderingParams,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn filetime_100ns(ft: FILETIME) -> u64 {
        (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
    }

    fn now_100ns() -> u64 {
        filetime_100ns(unsafe { GetSystemTimePreciseAsFileTime() })
    }

    fn text_utf16() -> Vec<u16> {
        SAMPLE.encode_utf16().collect()
    }

    pub fn fail(msg: &str) {
        let text: Vec<u16> = msg.encode_utf16().chain([0]).collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                w!("m0-win-render"),
                MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }

    pub fn run() -> Result<()> {
        let args: Vec<String> = std::env::args().collect();
        let mode = match args.get(1).map(String::as_str) {
            Some("empty") => Mode::Empty,
            Some("gdi") => Mode::Gdi,
            Some("d2d") => Mode::D2d,
            Some("dwrite-gdi") => Mode::DwriteGdi,
            _ => {
                fail("usage: m0-win-render <empty|gdi|d2d|dwrite-gdi> <out.jsonl> [idle_ms]");
                return Ok(());
            }
        };
        let out = args
            .get(2)
            .cloned()
            .unwrap_or_else(|| "render.jsonl".into());
        let idle_ms = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2000);

        let mut creation = FILETIME::default();
        let (mut a, mut b, mut c) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(GetCurrentProcess(), &mut creation, &mut a, &mut b, &mut c)?;
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }

        // Renderer factories are created before the window, as an app would.
        let (d2d, dw) = match mode {
            Mode::D2d => (Some(create_d2d()?), None),
            Mode::DwriteGdi => (None, Some(create_dwrite_gdi()?)),
            _ => (None, None),
        };
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                mode,
                out,
                idle_ms,
                t_created: filetime_100ns(creation),
                first_paint_ms: None,
                d2d,
                dw,
            })
        });

        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_IBEAM)?,
                lpszClassName: w!("SlashPadSpike"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(windows::core::Error::from_thread());
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("SlashPadSpike"),
                w!("SlashPad renderer spike"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                900,
                400,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let _ = UpdateWindow(hwnd);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    fn create_d2d() -> Result<D2d> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = create_format(&dwrite)?;
            Ok(D2d {
                factory,
                dwrite,
                format,
                target: None,
            })
        }
    }

    fn create_dwrite_gdi() -> Result<DwGdi> {
        unsafe {
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = create_format(&dwrite)?;
            let interop = dwrite.GetGdiInterop()?;
            let params = dwrite.CreateRenderingParams()?;
            Ok(DwGdi {
                dwrite,
                format,
                interop,
                params,
            })
        }
    }

    unsafe fn create_format(dwrite: &IDWriteFactory) -> Result<IDWriteTextFormat> {
        dwrite.CreateTextFormat(
            FONT,
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            FONT_PX,
            w!("en-us"),
        )
    }

    extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_PAINT => {
                    if let Err(e) = paint(hwnd) {
                        fail(&format!("paint failed: {e}"));
                    }
                    LRESULT(0)
                }
                WM_SIZE => {
                    STATE.with(|s| {
                        if let Some(Some(d2d)) = s.borrow_mut().as_mut().map(|st| st.d2d.as_mut()) {
                            d2d.target = None; // recreate at the new size on next paint
                        }
                    });
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    LRESULT(0)
                }
                WM_TIMER if wparam.0 == IDLE_TIMER => {
                    let _ = KillTimer(Some(hwnd), IDLE_TIMER);
                    report();
                    let _ = DestroyWindow(hwnd);
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

    unsafe fn paint(hwnd: HWND) -> Result<()> {
        let mode = STATE
            .with(|s| s.borrow().as_ref().map(|st| st.mode))
            .unwrap_or(Mode::Empty);
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc)?;
        match mode {
            Mode::Empty | Mode::Gdi => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                FillRect(hdc, &rc, HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut _));
                if mode == Mode::Gdi {
                    let dpi = GetDpiForWindow(hwnd).max(96) as f32;
                    let font = CreateFontW(
                        -((FONT_PX * dpi / 96.0) as i32),
                        0,
                        0,
                        0,
                        FW_NORMAL.0 as i32,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET,
                        OUT_DEFAULT_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        CLEARTYPE_QUALITY,
                        0,
                        FONT,
                    );
                    let old = SelectObject(hdc, font.into());
                    let mut text = text_utf16();
                    let mut r = rc;
                    r.left += 10;
                    r.top += 10;
                    DrawTextW(hdc, &mut text, &mut r, DT_LEFT | DT_NOPREFIX);
                    SelectObject(hdc, old);
                    let _ = DeleteObject(font.into());
                }
                let _ = EndPaint(hwnd, &ps);
            }
            Mode::D2d => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                STATE.with(|s| -> Result<()> {
                    let mut st = s.borrow_mut();
                    let Some(d2d) = st.as_mut().and_then(|st| st.d2d.as_mut()) else {
                        return Ok(());
                    };
                    if d2d.target.is_none() {
                        let dpi = GetDpiForWindow(hwnd).max(96) as f32;
                        let props = D2D1_RENDER_TARGET_PROPERTIES {
                            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                            pixelFormat: D2D1_PIXEL_FORMAT {
                                format: DXGI_FORMAT_UNKNOWN,
                                alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                            },
                            dpiX: dpi,
                            dpiY: dpi,
                            ..Default::default()
                        };
                        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                            hwnd,
                            pixelSize: D2D_SIZE_U {
                                width: (rc.right - rc.left) as u32,
                                height: (rc.bottom - rc.top) as u32,
                            },
                            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                        };
                        d2d.target = Some(d2d.factory.CreateHwndRenderTarget(&props, &hwnd_props)?);
                    }
                    if let Some(target) = &d2d.target {
                        let black = D2D1_COLOR_F {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        };
                        let white = D2D1_COLOR_F {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        };
                        let brush = target.CreateSolidColorBrush(&black, None)?;
                        let size = target.GetSize();
                        let layout = D2D_RECT_F {
                            left: 10.0,
                            top: 10.0,
                            right: size.width,
                            bottom: size.height,
                        };
                        target.BeginDraw();
                        target.Clear(Some(&white));
                        target.DrawText(
                            &text_utf16(),
                            &d2d.format,
                            &layout,
                            &brush,
                            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                        target.EndDraw(None, None)?;
                    }
                    let _ = &d2d.dwrite;
                    Ok(())
                })?;
                let _ = EndPaint(hwnd, &ps);
            }
            Mode::DwriteGdi => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                STATE.with(|s| -> Result<()> {
                    let st = s.borrow();
                    let Some(dw) = st.as_ref().and_then(|st| st.dw.as_ref()) else {
                        return Ok(());
                    };
                    let (w, h) = ((rc.right - rc.left) as u32, (rc.bottom - rc.top) as u32);
                    let target = dw.interop.CreateBitmapRenderTarget(Some(hdc), w, h)?;
                    let dpi = GetDpiForWindow(hwnd).max(96) as f32;
                    target.SetPixelsPerDip(dpi / 96.0)?;
                    let mem = target.GetMemoryDC();
                    let bg = RECT {
                        left: 0,
                        top: 0,
                        right: w as i32,
                        bottom: h as i32,
                    };
                    FillRect(mem, &bg, HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut _));
                    let text = text_utf16();
                    let layout = dw.dwrite.CreateTextLayout(
                        &text,
                        &dw.format,
                        w as f32 * 96.0 / dpi,
                        h as f32 * 96.0 / dpi,
                    )?;
                    let renderer: IDWriteTextRenderer = GdiTextRenderer {
                        target: target.clone(),
                        params: dw.params.clone(),
                    }
                    .into();
                    layout.Draw(None, &renderer, 10.0, 10.0)?;
                    BitBlt(hdc, 0, 0, w as i32, h as i32, Some(mem), 0, 0, SRCCOPY)?;
                    Ok(())
                })?;
                let _ = EndPaint(hwnd, &ps);
            }
        }

        let arm_timer = STATE.with(|s| {
            let mut st = s.borrow_mut();
            let st = st.as_mut()?;
            if st.first_paint_ms.is_some() {
                return None;
            }
            st.first_paint_ms = Some((now_100ns().saturating_sub(st.t_created)) as f64 / 10_000.0);
            Some(st.idle_ms)
        });
        if let Some(ms) = arm_timer {
            SetTimer(Some(hwnd), IDLE_TIMER, ms, None);
        }
        Ok(())
    }

    fn report() {
        let mut pmc = PROCESS_MEMORY_COUNTERS_EX2 {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
            ..Default::default()
        };
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut pmc as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                pmc.cb,
            )
        };
        STATE.with(|s| {
            let st = s.borrow();
            let Some(st) = st.as_ref() else { return };
            let mode = match st.mode {
                Mode::Empty => "empty",
                Mode::Gdi => "gdi",
                Mode::D2d => "d2d",
                Mode::DwriteGdi => "dwrite-gdi",
            };
            let line = if ok.is_ok() {
                format!(
                    "{{\"mode\":\"{mode}\",\"first_paint_ms\":{:.1},\"private_ws_kb\":{},\"private_commit_kb\":{},\"ws_kb\":{},\"peak_ws_kb\":{}}}\n",
                    st.first_paint_ms.unwrap_or(-1.0),
                    pmc.PrivateWorkingSetSize / 1024,
                    pmc.PrivateUsage / 1024,
                    pmc.WorkingSetSize / 1024,
                    pmc.PeakWorkingSetSize / 1024,
                )
            } else {
                format!("{{\"mode\":\"{mode}\",\"error\":\"GetProcessMemoryInfo failed\"}}\n")
            };
            let written = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&st.out)
                .and_then(|mut f| f.write_all(line.as_bytes()));
            if let Err(e) = written {
                fail(&format!("cannot write {}: {e}", st.out));
            }
        });
    }

    /// Minimal `IDWriteTextRenderer` that rasterises glyph runs into a GDI
    /// bitmap via `IDWriteBitmapRenderTarget` (no Direct3D involved).
    /// Colour emoji are drawn as monochrome outlines in this spike.
    #[implement(IDWriteTextRenderer)]
    struct GdiTextRenderer {
        target: IDWriteBitmapRenderTarget,
        params: IDWriteRenderingParams,
    }

    impl IDWritePixelSnapping_Impl for GdiTextRenderer_Impl {
        fn IsPixelSnappingDisabled(&self, _ctx: *const core::ffi::c_void) -> Result<BOOL> {
            Ok(false.into())
        }
        fn GetCurrentTransform(
            &self,
            _ctx: *const core::ffi::c_void,
            transform: *mut DWRITE_MATRIX,
        ) -> Result<()> {
            unsafe { self.target.GetCurrentTransform(transform) }
        }
        fn GetPixelsPerDip(&self, _ctx: *const core::ffi::c_void) -> Result<f32> {
            Ok(unsafe { self.target.GetPixelsPerDip() })
        }
    }

    impl IDWriteTextRenderer_Impl for GdiTextRenderer_Impl {
        fn DrawGlyphRun(
            &self,
            _ctx: *const core::ffi::c_void,
            x: f32,
            y: f32,
            mode: DWRITE_MEASURING_MODE,
            run: *const DWRITE_GLYPH_RUN,
            _desc: *const DWRITE_GLYPH_RUN_DESCRIPTION,
            _effect: Ref<IUnknown>,
        ) -> Result<()> {
            unsafe {
                self.target
                    .DrawGlyphRun(x, y, mode, run, &self.params, COLORREF(0), None)
            }
        }
        fn DrawUnderline(
            &self,
            _: *const core::ffi::c_void,
            _: f32,
            _: f32,
            _: *const DWRITE_UNDERLINE,
            _: Ref<IUnknown>,
        ) -> Result<()> {
            Ok(())
        }
        fn DrawStrikethrough(
            &self,
            _: *const core::ffi::c_void,
            _: f32,
            _: f32,
            _: *const DWRITE_STRIKETHROUGH,
            _: Ref<IUnknown>,
        ) -> Result<()> {
            Ok(())
        }
        fn DrawInlineObject(
            &self,
            _: *const core::ffi::c_void,
            _: f32,
            _: f32,
            _: Ref<IDWriteInlineObject>,
            _: BOOL,
            _: BOOL,
            _: Ref<IUnknown>,
        ) -> Result<()> {
            Ok(())
        }
    }
}
