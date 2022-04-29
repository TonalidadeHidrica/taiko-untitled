#[cfg(target_os = "macos")]
pub fn enable_momentum_scroll() {
    use libc::c_void;
    use objc::{
        class, msg_send,
        runtime::{Object, YES},
        sel, sel_impl,
    };

    const KEY: &str = "AppleMomentumScrollSupported";
    let string: *mut Object = unsafe { msg_send![class!(NSString), alloc] };
    let key: *mut Object = unsafe {
        msg_send![
            string,
            initWithBytes: KEY.as_ptr() as *const c_void
            length: KEY.len() as u32
            encoding: 4u32
        ]
    };
    let defaults: *mut Object = unsafe { msg_send![class!(NSUserDefaults), standardUserDefaults] };
    let _: () = unsafe { msg_send![defaults, setBool: YES forKey: key] };
    let _: () = unsafe { msg_send![key, release] };
}


#[cfg(not(target_os="macos"))]
pub fn enable_momentum_scroll() {
}
