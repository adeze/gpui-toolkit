//! GPUI element for a scene: paints software-rendered frames as GPUI images.

// Rust guideline compliant 2026-02-21

use crate::camera::Camera;
use crate::raster::render;
use crate::scene::Scene;
use gpui::{
    App, Bounds, Corners, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, RenderImage, Size, Style, Window, relative,
};
use image::{Frame, RgbaImage};
use std::cell::RefCell;
use std::panic;
use std::rc::Rc;
use std::sync::Arc;

/// Shared camera state for a scene element.
///
/// Hosts mutate it from mouse handlers (drag to orbit, wheel to dolly) and
/// request a repaint; the element reads it on every paint.
pub type SceneCamera = Rc<RefCell<Camera>>;

/// Creates shared camera state fitted to a scene's bounds.
#[must_use]
pub fn fitted_camera(scene: &Scene) -> SceneCamera {
    let camera = scene.bounds().map_or_else(Camera::default, Camera::fitted);
    Rc::new(RefCell::new(camera))
}

/// A GPUI element that fills its layout bounds with the rendered scene.
///
/// Rendering uses the deterministic software rasterizer, so the element
/// behaves the same in native windows, headless captures, and wasm hosts
/// that provide a GPUI image surface. Hosts that own a wgpu custom-draw
/// backend can instead upload [`crate::prepare_chunks`] output directly
/// and keep this element as the fallback path.
#[derive(Clone)]
pub struct SceneElement {
    scene: Scene,
    camera: SceneCamera,
}

impl SceneElement {
    /// Creates an element for a scene with shared camera state.
    pub fn new(scene: Scene, camera: SceneCamera) -> Self {
        Self { scene, camera }
    }

    /// Orbits the shared camera and marks it changed for the next paint.
    pub fn orbit(&self, d_azimuth: f32, d_elevation: f32) {
        self.camera.borrow_mut().orbit(d_azimuth, d_elevation);
    }

    /// Dollies the shared camera.
    pub fn dolly(&self, factor: f32) {
        self.camera.borrow_mut().dolly(factor);
    }
}

impl IntoElement for SceneElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SceneElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (
            window.request_layout(
                Style {
                    size: Size {
                        width: relative(1.0).into(),
                        height: relative(1.0).into(),
                    },
                    ..Default::default()
                },
                [],
                cx,
            ),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        (): &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        let width = f32::from(bounds.size.width).max(1.0) as u32;
        let height = f32::from(bounds.size.height).max(1.0) as u32;
        let camera = *self.camera.borrow();
        if let Ok(frame) = render(&self.scene, &camera, width, height)
            && let Some(rgba_image) = RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
        {
            let image = Arc::new(RenderImage::new(vec![Frame::new(rgba_image)]));
            let _ = window.paint_image(bounds, Corners::default(), image, 0, false);
        }
    }
}
