//! A `gdk::Paintable` that displays the video frame.
//!
//! The engine renders mpv into a private offscreen GL texture and hands it
//! over as a `GdkGLTexture`; GSK composites it like any other texture node.
//! This keeps Frame entirely out of GTK's own render target — GSK handles
//! placement, scaling, damage and texture recycling, so the video can never
//! fight the renderer over framebuffer layout (the displaced-band artifacts
//! reported on NVIDIA came from exactly that fight).

use std::cell::RefCell;

use gtk::gdk;
use gtk::gdk::subclass::prelude::*;
use gtk::prelude::*;

pub mod imp {
    use super::*;

    #[derive(Default)]
    pub struct VideoPaintable {
        pub(super) texture: RefCell<Option<gdk::Texture>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VideoPaintable {
        const NAME: &'static str = "FrameVideoPaintable";
        type Type = super::VideoPaintable;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for VideoPaintable {}

    impl PaintableImpl for VideoPaintable {
        fn intrinsic_width(&self) -> i32 {
            self.texture.borrow().as_ref().map(gdk::Texture::width).unwrap_or(0)
        }

        fn intrinsic_height(&self) -> i32 {
            self.texture.borrow().as_ref().map(gdk::Texture::height).unwrap_or(0)
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let tex = self.texture.borrow().clone();
            let Some(tex) = tex.as_ref() else {
                return;
            };
            if let Some(snapshot) = snapshot.downcast_ref::<gtk::Snapshot>() {
                snapshot.append_texture(
                    tex,
                    &gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
                );
            }
        }
    }
}

glib::wrapper! {
    pub struct VideoPaintable(ObjectSubclass<imp::VideoPaintable>) @implements gdk::Paintable;
}

impl VideoPaintable {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Publish a new frame; GSK re-snapshots on the next frame callback.
    pub fn set_texture(&self, tex: Option<&gdk::Texture>) {
        *self.imp().texture.borrow_mut() = tex.cloned();
        self.invalidate_contents();
    }
}

impl Default for VideoPaintable {
    fn default() -> Self {
        Self::new()
    }
}
