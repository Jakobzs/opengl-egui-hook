#![feature(once_cell)]

use anyhow::{anyhow, Result};
use detour::static_detour;
use egui::{Pos2, RawInput, Modifiers, CtxRef};
use painter::Painter;
use std::{
    ffi::{c_void, CString},
    mem,
};
use windows::{
    core::PCSTR,
    Win32::{
        Foundation::{GetLastError, BOOL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{WindowFromDC, HDC},
        System::{
            Console::AllocConsole,
            LibraryLoader::{GetModuleHandleA, GetProcAddress},
            SystemServices::DLL_PROCESS_ATTACH,
        },
        UI::{
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::{
                CallWindowProcW, SetWindowLongPtrW, GWL_WNDPROC, WHEEL_DELTA, WM_ACTIVATE, WM_CHAR,
                WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
                WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL,
                WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
                WM_XBUTTONDBLCLK, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1,
            },
        },
    },
};

mod painter;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    _module: HINSTANCE,
    call_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    if call_reason == DLL_PROCESS_ATTACH {
        BOOL::from(main().is_ok())
    } else {
        BOOL::from(true)
    }
}

fn create_debug_console() -> Result<()> {
    if !unsafe { AllocConsole() }.as_bool() {
        return Err(anyhow!(
            "Failed allocating console, GetLastError: {}",
            unsafe { GetLastError() }.0
        ));
    }

    Ok(())
}

fn get_module_library(
    module: &str,
    function: &str,
) -> Result<unsafe extern "system" fn() -> isize> {
    let module_cstring = CString::new(module).expect("module");
    let function_cstring = CString::new(function).expect("function");

    let h_instance = unsafe { GetModuleHandleA(PCSTR(module_cstring.as_ptr() as *mut _)) }?;

    let func = unsafe { GetProcAddress(h_instance, PCSTR(function_cstring.as_ptr() as *mut _)) };

    match func {
        Some(func) => Ok(func),
        None => Err(anyhow!(
            "Failed GetProcAddress, GetLastError: {}",
            unsafe { GetLastError() }.0
        )),
    }
}

static_detour! {
  pub static OpenGl32wglSwapBuffers: unsafe extern "system" fn(HDC) -> ();
}

fn egui_wnd_proc_impl(
    hwnd: HWND,
    umsg: u32,
    WPARAM(wparam): WPARAM,
    LPARAM(lparam): LPARAM,
) -> LRESULT {
    LRESULT(1)
    //unsafe { CallWindowProcW(ORIG_HWND, hwnd, umsg, WPARAM(wparam), LPARAM(lparam)) }
}

#[allow(non_snake_case)]
fn wndproc_hook(hWnd: HWND, uMsg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT {
    //println!("Msg is: {}", uMsg);

    if egui_wnd_proc_impl(hWnd, uMsg, wParam, lParam) == LRESULT(1) {
        return LRESULT(1);
    }

    println!("SKIPPP!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    unsafe { CallWindowProcW(ORIG_HWND, hWnd, uMsg, wParam, lParam) }
}

pub struct EguiInputState {
    pub pointer_pos: Pos2,
    pub input: RawInput,
    pub modifiers: Modifiers,
}

impl EguiInputState {
    pub fn new(input: RawInput) -> Self {
        EguiInputState {
            pointer_pos: Pos2::new(0f32, 0f32),
            input,
            modifiers: Modifiers::default(),
        }
    }
}

static mut INIT: bool = false;
static mut EGUI: Option<CtxRef> = None;
static mut EGUI_PAINTER: Option<Painter> = None;
static mut ORIG_HWND: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT> =
    None;

// Detour for wglSwapBuffers. This is the last call OpenGL makes, hence the immedate GUI should be drawn at this point
#[allow(non_snake_case)]
pub fn wglSwapBuffers_detour(dc: HDC) -> () {
    println!("Called wglSwapBuffers");

    if !unsafe { INIT } {
        let opengl_hwnd = unsafe { WindowFromDC(dc) };

        unsafe {
            ORIG_HWND = mem::transmute::<
                isize,
                Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
            >(SetWindowLongPtrW(
                opengl_hwnd,
                GWL_WNDPROC,
                wndproc_hook as isize,
            ))
        };

        let painter = Painter::new(1280, 1080);
        let egui_ctx = egui::CtxRef::default();

        unsafe { EGUI = Some(egui_ctx)};
        unsafe { EGUI_PAINTER = Some(painter)};

        unsafe { INIT = true };
    }

    if unsafe { INIT } {
        let egui_ctx = unsafe { &mut EGUI }.as_mut().unwrap();
        let painter = unsafe { &mut EGUI_PAINTER}.as_mut().unwrap();

        egui_ctx.begin_frame(RawInput::default());

        let mut amplitude = 0.0;

        egui::Window::new("Egui with GLFW").show(&egui_ctx, |ui| {
            ui.separator();
            ui.label("A simple sine wave plotted onto a GL texture then blitted to an egui managed Image.");
            ui.label(" ");
            ui.label(" ");
            
            ui.add(egui::Slider::new(&mut amplitude, 0.0..=50.0).text("Amplitude"));
            ui.label(" ");
            if ui.button("Quit").clicked() {
                println!("Clicked the button");
            }
        });

        let (egui_output, paint_cmds) = egui_ctx.end_frame();

        let paint_jobs = egui_ctx.tessellate(paint_cmds);
        
        painter.paint_jobs(
            None,
            paint_jobs,
            &egui_ctx.texture(),
            1.0,
        );

        /*let ui = imgui.frame();

        Window::new("Hello world")
            .size([300.0, 110.0], Condition::FirstUseEver)
            .build(&ui, || {
                ui.text("Hello world!");
                ui.text("こんにちは世界！");
                ui.text("This...is...imgui-rs!");
                ui.separator();
                let mouse_pos = ui.io().mouse_pos;
                ui.text(format!(
                    "Mouse Position: ({:.1},{:.1})",
                    mouse_pos[0], mouse_pos[1]
                ));
            });

        let rendererer = unsafe { &mut IMGUI_RENDERER }.as_mut().unwrap();
        rendererer.render(ui);

        println!("Mouse pos 0: {}", imgui.io().mouse_pos[0]);
        imgui.io_mut().mouse_pos[0] = 300.0;
        imgui.io_mut().mouse_pos[1] = 110.0;*/
    }

    unsafe { OpenGl32wglSwapBuffers.call(dc) }
}

pub type FnOpenGl32wglSwapBuffers = unsafe extern "system" fn(HDC) -> ();

fn main() -> Result<()> {
    create_debug_console()?;
    println!("Created debug console");

    let x = get_module_library("opengl32.dll", "wglSwapBuffers")?;
    let y: FnOpenGl32wglSwapBuffers = unsafe { mem::transmute(x) };
    unsafe { OpenGl32wglSwapBuffers.initialize(y, wglSwapBuffers_detour) }?;
    println!("Initialized detour");

    unsafe { OpenGl32wglSwapBuffers.enable() }?;
    println!("Enabled detour");

    Ok(())
}
