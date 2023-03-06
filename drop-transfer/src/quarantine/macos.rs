use std::{
    io::{Error, ErrorKind, Result},
    mem::transmute,
    os::raw::c_void,
    path::Path,
};

use core_foundation::{
    array::CFArray,
    base::{kCFAllocatorDefault, CFTypeID, CFTypeRef, OSStatus, TCFType},
    bundle::{CFBundleGetBundleWithIdentifier, CFBundleGetFunctionPointerForName},
    declare_TCFType, impl_TCFType,
    mach_port::CFAllocatorRef,
    string::{CFString, CFStringRef},
};

pub enum __MDItem {}

type MDItemRef = *mut __MDItem;
type MDItemSetAttribute = unsafe extern "C" fn(MDItemRef, CFStringRef, CFTypeRef) -> OSStatus;

// Declare and implement a Core Foundation type for metadata items to avoid
// having to manage memory manually.
declare_TCFType! {
    MDItem, MDItemRef
}
impl_TCFType!(MDItem, MDItemRef, MDItemGetTypeID);

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    // Metadata attribute for file origin.
    static kMDItemWhereFroms: CFStringRef;

    fn MDItemCreate(allocator: CFAllocatorRef, path: CFStringRef) -> MDItemRef;
    fn MDItemGetTypeID() -> CFTypeID;
}

/// Returns a pointer to a function in a specified code bundle or an error if
/// the bundle named `bundle_name` was not found or the bundle contains no
/// function `fn_name`.
///
/// * `bundle_name` - The name of a code bundle that should contain the
/// specified function.
/// * `fn_name` - The name of the function to load from the bundle.
fn bundle_fn_ptr(bundle_name: &str, fn_name: &str) -> Result<*const c_void> {
    let bundle = unsafe {
        CFBundleGetBundleWithIdentifier(CFString::new(bundle_name).as_concrete_TypeRef())
    };

    if bundle.is_null() {
        return Err(Error::new(
            ErrorKind::Unsupported,
            format!("Bundle {} not found", bundle_name),
        ));
    }

    let ptr = unsafe {
        CFBundleGetFunctionPointerForName(bundle, CFString::new(fn_name).as_concrete_TypeRef())
    };

    if ptr.is_null() {
        return Err(Error::new(
            ErrorKind::Unsupported,
            format!("{} not found in bundle {}", fn_name, bundle_name),
        ));
    }

    Ok(ptr)
}

impl super::PathExt for Path {
    fn quarantine(&self) -> Result<()> {
        // The reason this is loaded dynamically is that `MDItemSetAttribute()`
        // is not documented and its existence cannot be guaranteed, even though
        // it is already used by some major browsers to perform the same task
        // as this method.
        let ptr = bundle_fn_ptr("com.apple.Metadata", "MDItemSetAttribute")?;
        let item = unsafe {
            // Create a metadata item to later write the attribute in.
            MDItemCreate(
                kCFAllocatorDefault,
                CFString::new(&self.to_string_lossy()).as_concrete_TypeRef(),
            )
        };

        if item.is_null() {
            return Err(Error::new(
                ErrorKind::Other,
                format!("Failed to create metadata item"),
            ));
        }

        // https://developer.apple.com/documentation/corefoundation/1521153-cfrelease
        // The object is created under the “create rule”, as the name suggests,
        // so we want to release it afterwards to avoid leaks.
        let item = unsafe { MDItem::wrap_under_create_rule(item) };

        let location = CFString::new("meshnet");
        let array = CFArray::from_CFTypes(&[location.as_CFType()]);

        unsafe {
            let func: MDItemSetAttribute = transmute(ptr as *const c_void);
            // Actually set `com.apple.metadata:kMDItemWhereFroms` (yes, the
            // capitalization is supposed to be different between the _bundle_
            // and the _extended attribute_.
            let ret = func(
                item.as_concrete_TypeRef(),
                kMDItemWhereFroms,
                array.as_CFTypeRef(),
            );

            if ret != 0 {
                return Err(Error::new(
                    ErrorKind::Other,
                    format!("Setting metadata attribute failed with code {}", ret),
                ));
            }
        }

        Ok(())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::env::current_dir;

    use core_foundation::{
        array::CFArrayRef,
        base::{CFType, TCFTypeRef},
    };
    use tempfile::NamedTempFile;

    use super::*;

    type MDItemCopyAttribute = unsafe extern "C" fn(MDItemRef, CFStringRef) -> CFTypeRef;

    extern "C" {
        fn MDItemCopyAttribute(item: MDItemRef, name: CFStringRef) -> CFTypeRef;
    }

    #[test]
    fn add_metadata() -> Result<()> {
        let file = NamedTempFile::new_in(current_dir()?)?;
        let path = file.path();

        path.quarantine()?;

        let array = unsafe {
            let item = MDItemCreate(
                kCFAllocatorDefault,
                CFString::new(&path.to_string_lossy()).as_concrete_TypeRef(),
            );

            assert!(!item.is_null());

            let item = MDItem::wrap_under_create_rule(item);
            // Get all of the values for the `com.apple.metadata:kMDItemWhereFroms`
            // attribute.
            let attr = MDItemCopyAttribute(item.as_concrete_TypeRef(), kMDItemWhereFroms);

            assert!(!attr.is_null());

            CFArray::wrap_under_create_rule(CFArrayRef::from_void_ptr(attr))
        };

        // It is not impossible for something else in the system to write a value
        // into this attribute, so we check for existence, not equivalence.
        assert(
            array
                .iter()
                .filter(|elem| {
                    CFType::wrap_under_get_rule(*elem).downcast::<CFString>() == "meshnet"
                })
                .peekable()
                .peek()
                .is_some(),
        );

        Ok(())
    }
}
