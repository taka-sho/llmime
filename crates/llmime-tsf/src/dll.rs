//! DLL entry points required by Windows COM / TSF.

use windows::core::implement;
use windows::{
    core::{Interface, Result, GUID, HRESULT},
    Win32::{
        Foundation::{CLASS_E_CLASSNOTAVAILABLE, HINSTANCE},
        System::Com::{IClassFactory, IClassFactory_Impl},
    },
};

use crate::tsf::{TextInputProcessor, CLSID_LLMIME_TSF};

// ── DllMain ──────────────────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[no_mangle]
pub extern "system" fn DllMain(
    _hinst: HINSTANCE,
    _reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> bool {
    true
}

// ── DllGetClassObject ─────────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut std::ffi::c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return windows::core::HRESULT(-2147024809i32); // E_INVALIDARG
    }
    let clsid = unsafe { *rclsid };
    if clsid != CLSID_LLMIME_TSF {
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    let factory: IClassFactory = LlmimeClassFactory.into();
    match factory.query(&unsafe { *riid }, ppv) {
        Ok(()) => windows::core::S_OK,
        Err(e) => e.code(),
    }
}

// ── IClassFactory stub ────────────────────────────────────────────────────────

#[implement(IClassFactory)]
struct LlmimeClassFactory;

impl IClassFactory_Impl for LlmimeClassFactory_Impl {
    fn CreateInstance(
        &self,
        _punkouter: Option<&windows::core::IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut std::ffi::c_void,
    ) -> Result<()> {
        let processor = TextInputProcessor;
        let unk: windows::core::IUnknown = processor.into();
        unsafe { unk.query(&*riid, ppvobject) }
    }

    fn LockServer(&self, _flock: windows::Win32::Foundation::BOOL) -> Result<()> {
        Ok(())
    }
}
