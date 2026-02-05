//! CEF Render Handler
//!
//! Implements CEF's RenderHandler trait to capture off-screen rendered frames
//! and convert them to GPUI's RenderImage format. Frame buffer data is shared
//! via Arc<Mutex<RenderState>> (justified: large data, cross-thread).

use crate::events::{BrowserEvent, EventSender};
use cef::{
    rc::Rc as _, wrap_render_handler, Browser, ImplRenderHandler, PaintElementType, Rect,
    RenderHandler, ScreenInfo, WrapRenderHandler,
};
use gpui::RenderImage;
use image::{Frame, RgbaImage};
use parking_lot::Mutex;
use smallvec::SmallVec;
use std::sync::Arc;
use std::time::Instant;

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
    sender: EventSender,
}

impl OsrRenderHandler {
    pub fn new(state: Arc<Mutex<RenderState>>, sender: EventSender) -> Self {
        Self { state, sender }
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
            if type_ != PaintElementType::default() {
                return;
            }

            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }

            let width = width as u32;
            let height = height as u32;
            let buffer_size = (width * height * 4) as usize;

            // Read scale_factor with a brief lock, then release before doing
            // the heavy image construction work.
            let scale_factor = self.handler.state.lock().scale_factor;

            let pixel_data = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

            let image = match RgbaImage::from_raw(width, height, pixel_data.to_vec()) {
                Some(img) => img,
                None => {
                    log::error!("Failed to create RgbaImage from CEF buffer");
                    return;
                }
            };

            let render_image = Arc::new(
                RenderImage::new(SmallVec::from_elem(Frame::new(image), 1))
                    .with_scale_factor(scale_factor)
            );

            // Brief lock only to swap the frame pointer.
            self.handler.state.lock().current_frame = Some(render_image);

            let _ = self.handler.sender.send(BrowserEvent::FrameReady);
        }
    }
}

impl RenderHandlerBuilder {
    pub fn build(handler: OsrRenderHandler) -> cef::RenderHandler {
        Self::new(handler)
    }
}
