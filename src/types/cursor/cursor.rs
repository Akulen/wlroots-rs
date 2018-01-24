//! Wrapper for wlr_cursor

use std::ptr;

use wlroots_sys::{wlr_cursor, wlr_cursor_absolute_to_layout_coords,
                  wlr_cursor_attach_input_device, wlr_cursor_create, wlr_cursor_destroy,
                  wlr_cursor_detach_input_device, wlr_cursor_map_input_to_output,
                  wlr_cursor_map_input_to_region, wlr_cursor_map_to_output,
                  wlr_cursor_map_to_region, wlr_cursor_move, wlr_cursor_set_image,
                  wlr_cursor_set_surface, wlr_cursor_warp, wlr_cursor_warp_absolute};

use {Area, InputDevice, Output, Surface, XCursorImage};

#[derive(Debug)]
pub struct CursorBuilder {
    cursor: *mut wlr_cursor
}

#[derive(Debug)]
pub struct Cursor {
    cursor: *mut wlr_cursor
}

impl CursorBuilder {
    pub fn new() -> Option<Self> {
        unsafe {
            let cursor = wlr_cursor_create();
            if cursor.is_null() {
                None
            } else {
                Some(CursorBuilder { cursor: cursor })
            }
        }
    }

    /// Sets the image of the cursor to the image from the XCursor.
    pub fn set_cursor_image(self, image: &XCursorImage) -> Self {
        unsafe {
            let scale = 0.0;
            // NOTE Rationale for why lifetime isn't attached:
            //
            // wlr_cursor_set_image uses gl calls internally, which copies
            // the buffer and so it doesn't matter what happens to the
            // xcursor image after this call.
            wlr_cursor_set_image(self.cursor,
                                 image.buffer.as_ptr(),
                                 image.width as i32,
                                 image.width,
                                 image.height,
                                 image.hotspot_x as i32,
                                 image.hotspot_y as i32,
                                 scale)
        }
        self
    }

    pub(crate) fn build(self) -> Cursor {
        Cursor { cursor: self.cursor }
    }
}

impl Cursor {
    /// Get the coordinates the cursor is located at.
    pub fn coords(&self) -> (f64, f64) {
        unsafe { ((*self.cursor).x, (*self.cursor).y) }
    }

    /// Warp the cursor to the given x and y in layout coordinates. If x and y are
    /// out of the layout boundaries or constraints, no warp will happen.
    ///
    /// `dev` may be passed to respect device mapping constraints. If `dev` is None,
    /// device mapping constraints will be ignored.
    ///
    /// Returns true when the mouse warp was successful.
    pub fn warp(&mut self, dev: Option<&InputDevice>, x: f64, y: f64) -> bool {
        unsafe {
            let dev_ptr = dev.map(|input_device| input_device.as_ptr()).unwrap_or(ptr::null_mut());
            wlr_cursor_warp(self.cursor, dev_ptr, x, y)
        }
    }

    pub fn warp_absolute(&mut self, dev: Option<&InputDevice>, x_mm: f64, y_mm: f64) {
        unsafe {
            let dev_ptr = dev.map(|input_device| input_device.as_ptr()).unwrap_or(ptr::null_mut());
            wlr_cursor_warp_absolute(self.cursor, dev_ptr, x_mm, y_mm)
        }
    }

    /// Move the cursor in the direction of the given x and y coordinates.
    ///
    /// `dev` may be passed to respect device mapping constraints. If `dev` is None,
    /// device mapping constraints will be ignored.
    pub fn move_to(&mut self, dev: &InputDevice, delta_x: f64, delta_y: f64) {
        unsafe { wlr_cursor_move(self.cursor, dev.as_ptr(), delta_x, delta_y) }
    }

    // TODO Allow setting cursor images to arbitrary bytes,
    // just like in wlroots. Want to make sure that's safe though

    /// Sets the image of the cursor to the image from the XCursor.
    pub fn set_cursor_image(&mut self, image: &XCursorImage) {
        unsafe {
            let scale = 0.0;
            // NOTE Rationale for why lifetime isn't attached:
            //
            // wlr_cursor_set_image uses gl calls internally, which copies
            // the buffer and so it doesn't matter what happens to the
            // xcursor image after this call.
            wlr_cursor_set_image(self.cursor,
                                 image.buffer.as_ptr(),
                                 image.width as i32,
                                 image.width,
                                 image.height,
                                 image.hotspot_x as i32,
                                 image.hotspot_y as i32,
                                 scale)
        }
    }

    /// Set the cursor surface. The surface can be committed to update the cursor
    /// image. The surface position is substracted from the hotspot.
    ///
    /// A `None` surface commit hides the cursor.
    pub fn set_surface(&mut self, surface: Option<&Surface>, hotspot_x: i32, hotspot_y: i32) {
        unsafe {
            let surface_ptr = surface.map(|surface| surface.as_ptr())
                                     .unwrap_or(ptr::null_mut());
            wlr_cursor_set_surface(self.cursor, surface_ptr, hotspot_x, hotspot_y)
        }
    }

    // TODO Ensure the safety of these functions.
    // It's possible we need more handles floating about with checks...
    // or a different memory model -_-

    /// Attaches this input device to this cursor. The input device must be one of:
    ///
    /// - WLR_INPUT_DEVICE_POINTER
    /// - WLR_INPUT_DEVICE_TOUCH
    /// - WLR_INPUT_DEVICE_TABLET_TOOL
    ///
    /// TODO Make this impossible to mess up with using an enum
    /// Note that it's safe to use the wrong type.
    pub fn attach_input_device(&mut self, dev: &InputDevice) {
        unsafe { wlr_cursor_attach_input_device(self.cursor, dev.as_ptr()) }
    }

    /// Deattaches the input device from this cursor.
    pub fn deattach_input_device(&mut self, dev: &InputDevice) {
        unsafe { wlr_cursor_detach_input_device(self.cursor, dev.as_ptr()) }
    }

    // TODO Test what it means for the call to be "invalid". Segfault?

    /// Attaches this cursor to the given output, which must be among the outputs in
    /// the current output_layout for this cursor.
    ///
    /// This call is invalid for a cursor without an associated output layout.
    pub fn map_to_output(&mut self, output: &Output) {
        unsafe { wlr_cursor_map_to_output(self.cursor, output.as_ptr()) }
    }

    /// Maps all input from a specific input device to a given output.
    ///
    /// The input device must be attached to this cursor
    /// and the output must be among the outputs in the attached output layout.
    pub fn map_input_to_output(&mut self, dev: &InputDevice, output: &Output) {
        unsafe { wlr_cursor_map_input_to_output(self.cursor, dev.as_ptr(), output.as_ptr()) }
    }

    /// Maps this cursor to an arbitrary region on the associated
    /// wlr_output_layout.
    pub fn map_to_region(&mut self, mut area: Area) {
        unsafe { wlr_cursor_map_to_region(self.cursor, &mut area.0) }
    }

    /// Maps inputs from this input device to an arbitrary region on the associated
    /// wlr_output_layout.
    pub fn map_input_to_region(&mut self, dev: &InputDevice, mut area: Area) {
        unsafe { wlr_cursor_map_input_to_region(self.cursor, dev.as_ptr(), &mut area.0) }
    }

    /// Convert absolute coordinates to layout coordinates for the device.
    ///
    /// Coordinates are in (x, y).
    pub fn absolute_to_layout_coords(&mut self,
                                     dev: &InputDevice,
                                     x_mm: f64,
                                     y_mm: f64,
                                     width_mm: f64,
                                     height_mm: f64)
                                     -> (f64, f64) {
        unsafe {
            let (mut lx, mut ly) = (0.0, 0.0);
            wlr_cursor_absolute_to_layout_coords(self.cursor,
                                                 dev.as_ptr(),
                                                 x_mm,
                                                 y_mm,
                                                 width_mm,
                                                 height_mm,
                                                 &mut lx,
                                                 &mut ly);
            (lx, ly)
        }
    }

    pub(crate) unsafe fn as_ptr(&self) -> *mut wlr_cursor {
        self.cursor
    }
}

impl Drop for Cursor {
    fn drop(&mut self) {
        unsafe { wlr_cursor_destroy(self.cursor) }
    }
}
