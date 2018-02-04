//! TODO Documentation

use std::{panic, ptr};
use std::ffi::CStr;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

use libc::c_float;
use wayland_sys::server::WAYLAND_SERVER_HANDLE;
use wlroots_sys::{wl_list, wl_output_subpixel, wl_output_transform, wlr_output,
                  wlr_output_effective_resolution, wlr_output_enable, wlr_output_get_gamma_size,
                  wlr_output_make_current, wlr_output_mode, wlr_output_set_custom_mode,
                  wlr_output_set_fullscreen_surface, wlr_output_set_gamma, wlr_output_set_mode,
                  wlr_output_set_position, wlr_output_set_scale, wlr_output_set_transform,
                  wlr_output_swap_buffers};

use super::output_layout::OutputLayoutHandle;
use super::output_mode::OutputMode;
use errors::{UpgradeHandleErr, UpgradeHandleResult};
use utils::c_to_rust_string;

pub type Subpixel = wl_output_subpixel;
pub type Transform = wl_output_transform;

use {Origin, Size, Surface, SurfaceHandle};

struct OutputState {
    handle: Weak<AtomicBool>,
    layout_handle: Option<OutputLayoutHandle>
}

#[derive(Debug)]
pub struct Output {
    /// The structure that ensures weak handles to this structure are still alive.
    ///
    /// They contain weak handles, and will safely not use dead memory when this
    /// is freed by wlroots.
    ///
    /// If this is `None`, then this is from an upgraded `OutputHandle`, and
    /// the operations are **unchecked**.
    /// This is means safe operations might fail, but only if you use the unsafe
    /// marked function `upgrade` on a `OutputHandle`.
    liveliness: Option<Rc<AtomicBool>>,
    /// The output ptr that refers to this `Output`
    output: *mut wlr_output
}

/// A wrapper around a wlr_output.
#[derive(Debug)]
pub struct OutputHandle {
    /// The Rc that ensures that this handle is still alive.
    ///
    /// When wlroots deallocates the pointer associated with this handle,
    /// this can no longer be used.
    handle: Weak<AtomicBool>,
    /// The output ptr that refers to this `Output`
    output: *mut wlr_output
}

impl Output {
    /// Just like `std::clone::Clone`, but unsafe.
    ///
    /// # Unsafety
    /// You can create a reference leak using this method very easily,
    /// and it could make it so that weak handles to an output are dangling.
    ///
    /// This exists due to an issue in output_manager.rs that might be fixed
    /// with NLL, so if this is no longer necessary it should be removed asap.
    pub(crate) unsafe fn clone(&self) -> Output {
        Output { liveliness: self.liveliness.clone(),
                 output: self.output }
    }

    /// Makes a new `Output` from a `wlr_output`.
    ///
    /// # Safety
    /// This creates a totally new Output (e.g with its own reference count)
    /// so only do this once per `wlr_output`!
    pub(crate) unsafe fn new(output: *mut wlr_output) -> Self {
        (*output).data = ptr::null_mut();
        let liveliness = Rc::new(AtomicBool::new(false));
        let handle = Rc::downgrade(&liveliness);
        let state = Box::new(OutputState { handle,
                                           layout_handle: None });
        (*output).data = Box::into_raw(state) as *mut _;
        Output { liveliness: Some(liveliness),
                 output }
    }

    pub(crate) unsafe fn set_output_layout(&mut self, layout_handle: Option<OutputLayoutHandle>) {
        self.remove_from_output_layout();
        let user_data = self.user_data();
        if user_data.is_null() {
            return
        }
        let mut data = Box::from_raw(user_data);
        data.layout_handle = layout_handle;
        (*self.output).data = Box::into_raw(data) as *mut _;
    }

    unsafe fn user_data(&mut self) -> *mut OutputState {
        (*self.output).data as *mut _
    }

    /// Used to clear the pointer to an OutputLayout when the OutputLayout
    /// removes this Output from its listing.
    pub(crate) unsafe fn clear_output_layout_data(&mut self) {
        let user_data = self.user_data();
        if user_data.is_null() {
            return
        }
        let mut data = Box::from_raw(user_data);
        data.layout_handle = None;
        (*self.output).data = Box::into_raw(data) as *mut _;
    }

    /// Remove this Output from an OutputLayout, if it is part of an
    /// OutputLayout.
    pub(crate) unsafe fn remove_from_output_layout(&mut self) {
        let output_data = self.user_data();
        if output_data.is_null() {
            return
        }
        // Remove output from previous output layout.
        if let Some(mut layout_handle) = (*output_data).layout_handle.take() {
            match layout_handle.run(|layout| layout.remove(self)) {
                Ok(_) | Err(UpgradeHandleErr::AlreadyDropped) => self.clear_output_layout_data(),
                Err(UpgradeHandleErr::AlreadyBorrowed) => {
                    panic!("Could not add OutputLayout to Output user data!")
                }
            }
        }
    }

    /// Gets the OutputLayout this Output is a part of, if it is part
    /// of an OutputLayout.
    ///
    /// # Safety
    /// Note that this isn't exposed to user space, as they could easily
    /// create two mutable pointers to the same structure. We keep it internally
    /// though because we use it during the cleanup process.
    pub(crate) unsafe fn layout(&mut self) -> Option<OutputLayoutHandle> {
        let data = self.user_data();
        if data.is_null() {
            None
        } else {
            (*data).layout_handle.clone()
        }
    }

    /// Sets the best modesetting for an output.
    ///
    /// NOTE You _cannot_ call this when the output will be removed.
    ///
    /// I'm still marking it as safe though because we protect against that
    /// action in the output destruction callback.
    pub fn choose_best_mode(&mut self) {
        unsafe {
            let modes = &mut (*self.output).modes as *mut wl_list;
            let length = ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_list_length, modes as _);
            if length > 0 {
                // TODO Better logging
                wlr_log!(L_DEBUG, "output added {:?}", self);
                let first_mode_ptr: *mut wlr_output_mode;
                first_mode_ptr =
                    container_of!(&mut (*(*modes).prev) as *mut _, wlr_output_mode, link);
                wlr_output_set_mode(self.as_ptr(), first_mode_ptr);
            }
        }
    }

    // TODO Could we pass an output mode from the wrong output here?
    // What will happen?

    /// Set this to be the current mode for the Output.
    pub fn set_mode(&mut self, mode: OutputMode) -> bool {
        unsafe { wlr_output_set_mode(self.output, mode.as_ptr()) }
    }

    /// Set a custom mode for this output.
    pub fn set_custom_mode(&mut self, size: Size, refresh: i32) -> bool {
        unsafe { wlr_output_set_custom_mode(self.output, size.width, size.height, refresh) }
    }

    /// Gets the name of the output in UTF-8.
    pub fn name(&self) -> String {
        unsafe {
            CStr::from_ptr((*self.output).name.as_ptr()).to_string_lossy()
                                                        .into_owned()
        }
    }

    /// Gets the make of the output in UTF-8.
    pub fn make(&self) -> String {
        unsafe {
            c_to_rust_string((*self.output).make.as_ptr()).expect("Could not parse make as UTF-8")
        }
    }

    /// Gets the model of the output in UTF-8.
    pub fn model(&self) -> String {
        unsafe {
            c_to_rust_string((*self.output).model.as_ptr()).expect("Could not parse model as UTF-8")
        }
    }

    /// Gets the serial of the output in UTF-8.
    pub fn serial(&self) -> String {
        unsafe {
            c_to_rust_string((*self.output).serial.as_ptr()).expect("Could not parse serial as \
                                                                     UTF-8")
        }
    }

    /// Determines if the output is enabled or not.
    pub fn enabled(&self) -> bool {
        unsafe { (*self.output).enabled }
    }

    /// Get the scale of the output
    pub fn scale(&self) -> c_float {
        unsafe { (*self.output).scale }
    }

    /// Determines if the output should have its buffers swapped or not.
    pub fn needs_swap(&self) -> bool {
        unsafe { (*self.output).needs_swap }
    }

    /// Get the refresh rate of the output.
    pub fn refresh_rate(&self) -> i32 {
        unsafe { (*self.output).refresh }
    }

    pub fn current_mode(&self) -> Option<OutputMode> {
        unsafe {
            if (*self.output).current_mode.is_null() {
                None
            } else {
                Some(OutputMode::new((*self.output).current_mode))
            }
        }
    }

    pub fn fullscreen_surface(&self) -> Option<SurfaceHandle> {
        unsafe {
            if (*self.output).fullscreen_surface.is_null() {
                None
            } else {
                Some(SurfaceHandle::from_ptr((*self.output).fullscreen_surface))
            }
        }
    }

    /// Gets the output position in layout space reported to clients.
    pub fn layout_space_pos(&self) -> (i32, i32) {
        unsafe { ((*self.output).lx, (*self.output).ly) }
    }

    /// Get subpixel information about the output.
    pub fn subpixel(&self) -> Subpixel {
        unsafe { (*self.output).subpixel }
    }

    /// Get the transform information about the output.
    pub fn get_transform(&self) -> Transform {
        unsafe { (*self.output).transform }
    }

    pub fn make_current(&mut self) {
        unsafe { wlr_output_make_current(self.output) }
    }

    pub fn swap_buffers(&mut self) {
        unsafe { wlr_output_swap_buffers(self.output) }
    }

    /// Get the dimensions of the output as (width, height).
    pub fn dimensions(&self) -> (i32, i32) {
        unsafe { ((*self.output).width, (*self.output).height) }
    }

    /// Get the physical dimensions of the output as (width, height).
    pub fn physical_dimensions(&self) -> (i32, i32) {
        unsafe { ((*self.output).phys_width, (*self.output).phys_height) }
    }

    pub fn effective_resolution(&self) -> (i32, i32) {
        unsafe {
            let (mut x, mut y) = (0, 0);
            wlr_output_effective_resolution(self.output, &mut x, &mut y);
            (x, y)
        }
    }

    pub fn transform_matrix(&self) -> [c_float; 16] {
        unsafe { (*self.output).transform_matrix }
    }

    pub fn transform(&mut self, transform: Transform) {
        unsafe {
            wlr_output_set_transform(self.output, transform);
        }
    }

    /// Get the modes associated with this output.
    ///
    /// Note that some backends may have zero modes.
    pub fn modes(&self) -> Vec<OutputMode> {
        unsafe {
            let mut result = vec![];
            wl_list_for_each!((*self.output).modes, link, (mode: wlr_output_mode) => {
                result.push(OutputMode::new(mode))
            });
            result
        }
    }

    /// Enables or disables an output.
    pub fn enable(&mut self, enable: bool) {
        unsafe { wlr_output_enable(self.output, enable) }
    }

    /// Sets the gamma based on the size.
    pub fn set_gamma(&mut self, size: u32, mut r: u16, mut g: u16, mut b: u16) {
        unsafe { wlr_output_set_gamma(self.output, size, &mut r, &mut g, &mut b) }
    }

    /// Get the gamma size.
    pub fn get_gamma_size(&self) -> u32 {
        unsafe { wlr_output_get_gamma_size(self.output) }
    }

    /// Set the fullscreen surface for this output.
    pub fn set_fullscreen_surface(&mut self, surface: &mut Surface) {
        unsafe { wlr_output_set_fullscreen_surface(self.output, surface.as_ptr()) }
    }

    /// Sets the position of this output.
    pub fn set_position(&mut self, origin: Origin) {
        unsafe { wlr_output_set_position(self.output, origin.x, origin.y) }
    }

    /// Set the scale applied to this output.
    pub fn set_scale(&mut self, scale: c_float) {
        unsafe { wlr_output_set_scale(self.output, scale) }
    }

    pub(crate) unsafe fn as_ptr(&self) -> *mut wlr_output {
        self.output
    }

    /// Creates a weak reference to an `Output`.
    ///
    /// # Panics
    /// If this `Output` is a previously upgraded `OutputHandle`,
    /// then this function will panic.
    pub fn weak_reference(&self) -> OutputHandle {
        let arc = self.liveliness.as_ref()
                      .expect("Cannot downgrade a previously upgraded OutputHandle");
        OutputHandle { handle: Rc::downgrade(arc),
                       output: self.output }
    }

    unsafe fn from_handle(handle: &OutputHandle) -> Self {
        Output { liveliness: None,
                 output: handle.as_ptr() }
    }

    /// Manually set the lock used to determine if a double-borrow is
    /// occuring on this structure.
    ///
    /// # Panics
    /// Panics when trying to set the lock on an upgraded handle.
    pub(crate) unsafe fn set_lock(&self, val: bool) {
        self.liveliness.as_ref()
            .expect("Tried to set lock on borrowed Output")
            .store(val, Ordering::Release);
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // NOTE
        // We do _not_ need to call wlr_output_destroy for the output
        // That is handled by the backend automatically
        match self.liveliness {
            None => return,
            Some(ref liveliness) => {
                if Rc::strong_count(liveliness) == 1 {
                    wlr_log!(L_DEBUG, "Dropped output {:p}", self.output);
                    let weak_count = Rc::weak_count(liveliness);
                    if weak_count > 0 {
                        wlr_log!(L_DEBUG,
                                 "Still {} weak pointers to Output {:p}",
                                 weak_count,
                                 self.output);
                    }
                } else {
                    return
                }
            }
        }
        // TODO Move back up in the some after NLL is a thing.
        unsafe {
            self.remove_from_output_layout();
        }
    }
}

impl OutputHandle {
    /// Creates an OutputHandle from the raw pointer, using the saved
    /// user data to recreate the memory model.
    pub(crate) unsafe fn from_ptr(output: *mut wlr_output) -> Self {
        let data = Box::from_raw((*output).data as *mut OutputState);
        let handle = data.handle.clone();
        (*output).data = Box::into_raw(data) as *mut _;
        OutputHandle { handle, output }
    }

    /// Upgrades the output handle to a reference to the backing `Output`.
    ///
    /// # Unsafety
    /// This function is unsafe, because it creates an unbound `Output`
    /// which may live forever..
    /// But no output lives forever and might be disconnected at any time.
    pub(crate) unsafe fn upgrade(&self) -> UpgradeHandleResult<Output> {
        self.handle.upgrade()
            .ok_or(UpgradeHandleErr::AlreadyDropped)
            // NOTE
            // We drop the Rc here because having two would allow a dangling
            // pointer to exist!
            .and_then(|check| {
                let output = Output::from_handle(self);
                if check.load(Ordering::Acquire) {
                    return Err(UpgradeHandleErr::AlreadyBorrowed)
                }
                check.store(true, Ordering::Release);
                Ok(output)
            })
    }

    /// Run a function on the referenced Output, if it still exists
    ///
    /// Returns the result of the function, if successful
    ///
    /// # Safety
    /// By enforcing a rather harsh limit on the lifetime of the output
    /// to a short lived scope of an anonymous function,
    /// this function ensures the Output does not live longer
    /// than it exists.
    ///
    /// # Panics
    /// This function will panic if multiple mutable borrows are detected.
    /// This will happen if you call `upgrade` directly within this callback,
    /// or if you run this function within the another run to the same `Output`.
    ///
    /// So don't nest `run` calls and everything will be ok :).
    pub fn run<F, R>(&mut self, runner: F) -> UpgradeHandleResult<R>
        where F: FnOnce(&mut Output) -> R
    {
        let mut output = unsafe { self.upgrade()? };
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| runner(&mut output)));
        self.handle.upgrade().map(|check| {
                                      // Sanity check that it hasn't been tampered with.
                                      if !check.load(Ordering::Acquire) {
                                          wlr_log!(L_ERROR,
                                                   "After running output callback, mutable lock \
                                                    was false for: {:?}",
                                                   output);
                                          panic!("Lock in incorrect state!");
                                      }
                                      check.store(false, Ordering::Release);
                                  });
        match res {
            Ok(res) => Ok(res),
            Err(err) => panic::resume_unwind(err)
        }
    }

    pub(crate) unsafe fn as_ptr(&self) -> *mut wlr_output {
        self.output
    }
}

impl PartialEq for OutputHandle {
    fn eq(&self, other: &OutputHandle) -> bool {
        self.output == other.output
    }
}

impl Eq for OutputHandle {}
