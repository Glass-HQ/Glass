//! CEF Render Handler
//!
//! Implements CEF's RenderHandler trait to capture off-screen rendered frames
//! and convert them to GPUI's RenderImage format.

use cef::{
    rc::Rc as _, wrap_render_handler, Browser, ImplRenderHandler, PaintElementType, Rect,
    RenderHandler, ScreenInfo, WrapRenderHandler,
};
use gpui::RenderImage;
use image::{Frame, RgbaImage};
use parking_lot::Mutex;
use smallvec::SmallVec;
use std::sync::Arc;

pub struct RenderState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub current_frame: Option<Arc<RenderImage>>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            scale_factor: 1.0,
            current_frame: None,
        }
    }
}

#[derive(Clone)]
pub struct OsrRenderHandler {
    state: Arc<Mutex<RenderState>>,
}

impl OsrRenderHandler {
    pub fn new(state: Arc<Mutex<RenderState>>) -> Self {
        Self { state }
    }
}

wrap_render_handler! {
    pub struct RenderHandlerBuilder {
        handler: OsrRenderHandler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let state = self.handler.state.lock();
                rect.x = 0;
                rect.y = 0;
                rect.width = state.width as i32;
                rect.height = state.height as i32;
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            if let Some(info) = screen_info {
                let state = self.handler.state.lock();
                info.device_scale_factor = state.scale_factor;
                info.rect.x = 0;
                info.rect.y = 0;
                info.rect.width = state.width as i32;
                info.rect.height = state.height as i32;
                info.available_rect = info.rect.clone();
                info.depth = 32;
                info.depth_per_component = 8;
                info.is_monochrome = 0;
                return 1;
            }
            0
        }

        fn screen_point(
            &self,
            _browser: Option<&mut Browser>,
            view_x: ::std::os::raw::c_int,
            view_y: ::std::os::raw::c_int,
            screen_x: Option<&mut ::std::os::raw::c_int>,
            screen_y: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let Some(screen_x) = screen_x {
                *screen_x = view_x;
            }
            if let Some(screen_y) = screen_y {
                *screen_y = view_y;
            }
            1
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            // PaintElementType::PET_VIEW is the main view (0)
            // PaintElementType::PET_POPUP is for popups (1)
            if type_ != PaintElementType::default() {
                return;
            }

            if buffer.is_null() || width <= 0 || height <= 0 {
                log::warn!("Invalid paint buffer: null={}, width={}, height={}",
                    buffer.is_null(), width, height);
                return;
            }

            let width = width as u32;
            let height = height as u32;
            let buffer_size = (width * height * 4) as usize;

            // CEF outputs BGRA format, which is exactly what GPUI's RenderImage expects
            let bgra_data = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

            // RgbaImage is used as a container but GPUI interprets it as BGRA
            let image = match RgbaImage::from_raw(width, height, bgra_data.to_vec()) {
                Some(img) => img,
                None => {
                    log::error!("Failed to create RgbaImage from CEF buffer");
                    return;
                }
            };

            let frame = Frame::new(image);

            let mut state = self.handler.state.lock();
            let scale_factor = state.scale_factor;
            let render_image = Arc::new(
                RenderImage::new(SmallVec::from_elem(frame, 1))
                    .with_scale_factor(scale_factor)
            );
            state.current_frame = Some(render_image);
        }
    }
}

impl RenderHandlerBuilder {
    pub fn build(handler: OsrRenderHandler) -> cef::RenderHandler {
        Self::new(handler)
    }
}
