//! ITextStoreACP — exposes the composition document to TSF.

use windows::{
    core::{implement, IUnknown, Result, GUID, HRESULT},
    Win32::{
        Foundation::{BOOL, FALSE, HWND, POINT, RECT, TRUE},
        UI::TextServices::{
            ITextStoreACP, ITextStoreACPSink, ITextStoreACP_Impl, TS_ATTRID, TS_ATTRVAL,
            TS_RUNINFO, TS_SELECTION_ACP, TS_SS_NOHIDE, TS_STATUS, TS_ST_CORRECTION, TS_TEXTCHANGE,
        },
    },
};

use crate::state::SharedState;

// E_NOTIMPL
const E_NOTIMPL: HRESULT = HRESULT(-2147467263i32);

#[implement(ITextStoreACP)]
pub struct LlmimeTextStore {
    hwnd: HWND,
    state: SharedState,
    sink: std::cell::Cell<Option<ITextStoreACPSink>>,
    sink_mask: std::cell::Cell<u32>,
}

impl LlmimeTextStore {
    pub fn new(hwnd: HWND, state: SharedState) -> Self {
        Self {
            hwnd,
            state,
            sink: std::cell::Cell::new(None),
            sink_mask: std::cell::Cell::new(0),
        }
    }
}

impl ITextStoreACP_Impl for LlmimeTextStore_Impl {
    fn AdviseSink(&self, _riid: *const GUID, punk: Option<&IUnknown>, dwmask: u32) -> Result<()> {
        if let Some(unk) = punk {
            let sink: ITextStoreACPSink = unk.cast()?;
            self.sink.set(Some(sink));
            self.sink_mask.set(dwmask);
        }
        Ok(())
    }

    fn UnadviseSink(&self, _punk: Option<&IUnknown>) -> Result<()> {
        self.sink.set(None);
        self.sink_mask.set(0);
        Ok(())
    }

    fn RequestLock(&self, dwlockflags: u32) -> Result<HRESULT> {
        if let Some(sink) = self.sink.take() {
            let hr = unsafe { sink.OnLockGranted(dwlockflags) };
            self.sink.set(Some(sink));
            hr?;
        }
        Ok(windows::core::S_OK)
    }

    fn GetStatus(&self) -> Result<TS_STATUS> {
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: TS_SS_NOHIDE.0,
        })
    }

    fn QueryInsert(&self, acpteststart: i32, acptestend: i32, _cch: u32) -> Result<(i32, i32)> {
        Ok((acpteststart, acptestend))
    }

    fn GetSelection(
        &self,
        _ulindex: u32,
        _ulcount: u32,
        pselection: *mut TS_SELECTION_ACP,
        pcfetched: *mut u32,
    ) -> Result<()> {
        let state = self.state.lock().unwrap();
        let cursor = state.cursor;
        unsafe {
            if !pselection.is_null() {
                (*pselection).acpStart = cursor;
                (*pselection).acpEnd = cursor;
                (*pselection).style.fInterimChar = FALSE;
                (*pselection).style.ase = TS_ST_CORRECTION;
            }
            if !pcfetched.is_null() {
                *pcfetched = 1;
            }
        }
        Ok(())
    }

    fn SetSelection(&self, _ulcount: u32, pselection: *const TS_SELECTION_ACP) -> Result<()> {
        if !pselection.is_null() {
            let mut state = self.state.lock().unwrap();
            state.cursor = unsafe { (*pselection).acpEnd };
        }
        Ok(())
    }

    fn GetText(
        &self,
        acpstart: i32,
        acpend: i32,
        pchplain: *mut u16,
        cchplainreq: u32,
        pcchplainret: *mut u32,
        _prgruninfo: *mut TS_RUNINFO,
        _crunginfo: u32,
        _pcruninfo: *mut u32,
        pacpnext: *mut i32,
    ) -> Result<()> {
        let state = self.state.lock().unwrap();
        let text = &state.preedit;
        let start = (acpstart as usize).min(text.len());
        let end = if acpend < 0 {
            text.len()
        } else {
            (acpend as usize).min(text.len())
        };
        let slice = &text[start..end];
        let copy_len = (slice.len() as u32).min(cchplainreq) as usize;

        unsafe {
            if !pchplain.is_null() && copy_len > 0 {
                std::ptr::copy_nonoverlapping(slice.as_ptr(), pchplain, copy_len);
            }
            if !pcchplainret.is_null() {
                *pcchplainret = copy_len as u32;
            }
            if !pacpnext.is_null() {
                *pacpnext = (start + copy_len) as i32;
            }
        }
        Ok(())
    }

    fn SetText(
        &self,
        _dwflags: u32,
        acpstart: i32,
        acpend: i32,
        pchtext: *const u16,
        cch: u32,
    ) -> Result<TS_TEXTCHANGE> {
        let mut state = self.state.lock().unwrap();
        let new_text: Vec<u16> = if pchtext.is_null() || cch == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(pchtext, cch as usize).to_vec() }
        };

        let old_len = state.preedit.len() as i32;
        let start = acpstart.min(old_len);
        let end = acpend.min(old_len);

        let mut new_buf = Vec::new();
        new_buf.extend_from_slice(&state.preedit[..start as usize]);
        new_buf.extend_from_slice(&new_text);
        new_buf.extend_from_slice(&state.preedit[end as usize..]);
        state.preedit = new_buf;
        state.cursor = start + new_text.len() as i32;

        Ok(TS_TEXTCHANGE {
            acpStart: start,
            acpOldEnd: end,
            acpNewEnd: start + new_text.len() as i32,
        })
    }

    fn GetFormattedText(
        &self,
        _acpstart: i32,
        _acpend: i32,
    ) -> Result<windows::Win32::System::Com::IDataObject> {
        Err(windows::core::Error::from(E_NOTIMPL))
    }

    fn GetEmbedded(
        &self,
        _acppos: i32,
        _rguidservice: *const GUID,
        _riid: *const GUID,
    ) -> Result<windows::core::IUnknown> {
        Err(windows::core::Error::from(E_NOTIMPL))
    }

    fn QueryInsertEmbedded(
        &self,
        _pguidservice: *const GUID,
        _pformatetc: *const windows::Win32::System::Com::FORMATETC,
    ) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn InsertEmbedded(
        &self,
        _dwflags: u32,
        _acpstart: i32,
        _acpend: i32,
        _pdataobject: Option<&windows::Win32::System::Com::IDataObject>,
    ) -> Result<TS_TEXTCHANGE> {
        Err(windows::core::Error::from(E_NOTIMPL))
    }

    fn InsertTextAtSelection(
        &self,
        dwflags: u32,
        pchtext: *const u16,
        cch: u32,
        pacpstart: *mut i32,
        pacpend: *mut i32,
        pchange: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        let new_text: Vec<u16> = if pchtext.is_null() || cch == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(pchtext, cch as usize).to_vec() }
        };

        let mut state = self.state.lock().unwrap();
        let pos = state.cursor;
        let insert_start = pos.min(state.preedit.len() as i32);

        let change = TS_TEXTCHANGE {
            acpStart: insert_start,
            acpOldEnd: insert_start,
            acpNewEnd: insert_start + new_text.len() as i32,
        };

        // TF_IAS_QUERYONLY: only query, don't modify
        const TF_IAS_QUERYONLY: u32 = 1;
        if dwflags & TF_IAS_QUERYONLY == 0 {
            let mut new_buf = Vec::new();
            new_buf.extend_from_slice(&state.preedit[..insert_start as usize]);
            new_buf.extend_from_slice(&new_text);
            new_buf.extend_from_slice(&state.preedit[insert_start as usize..]);
            state.preedit = new_buf;
            state.cursor = change.acpNewEnd;
        }

        unsafe {
            if !pacpstart.is_null() {
                *pacpstart = change.acpStart;
            }
            if !pacpend.is_null() {
                *pacpend = change.acpNewEnd;
            }
            if !pchange.is_null() {
                *pchange = change;
            }
        }
        Ok(())
    }

    fn InsertEmbeddedAtSelection(
        &self,
        _dwflags: u32,
        _pdataobject: Option<&windows::Win32::System::Com::IDataObject>,
        _pacpstart: *mut i32,
        _pacpend: *mut i32,
        _pchange: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        Err(windows::core::Error::from(E_NOTIMPL))
    }

    fn RequestSupportedAttrs(
        &self,
        _dwflags: u32,
        _cfilterattrs: u32,
        _pafilterattrs: *const TS_ATTRID,
    ) -> Result<()> {
        Ok(())
    }

    fn RequestAttrsAtPosition(
        &self,
        _acppos: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const TS_ATTRID,
        _dwflags: u32,
    ) -> Result<()> {
        Ok(())
    }

    fn RequestAttrsTransitioningAtPosition(
        &self,
        _acppos: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const TS_ATTRID,
        _dwflags: u32,
    ) -> Result<()> {
        Ok(())
    }

    fn FindNextAttrTransition(
        &self,
        _acpstart: i32,
        _acphalt: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const TS_ATTRID,
        _dwflags: u32,
        pacpnext: *mut i32,
        _pffound: *mut BOOL,
        _plfoundoffset: *mut i32,
    ) -> Result<()> {
        unsafe {
            if !pacpnext.is_null() {
                *pacpnext = 0;
            }
        }
        Ok(())
    }

    fn RetrieveRequestedAttrs(
        &self,
        _ulcount: u32,
        _paattrvals: *mut TS_ATTRVAL,
        pcfetched: *mut u32,
    ) -> Result<()> {
        unsafe {
            if !pcfetched.is_null() {
                *pcfetched = 0;
            }
        }
        Ok(())
    }

    fn GetEndACP(&self) -> Result<i32> {
        let state = self.state.lock().unwrap();
        Ok(state.preedit.len() as i32)
    }

    fn GetActiveView(&self) -> Result<u32> {
        Ok(0)
    }

    fn GetACPFromPoint(&self, _vcview: u32, _ptscreen: *const POINT, _dwflags: u32) -> Result<i32> {
        let state = self.state.lock().unwrap();
        Ok(state.cursor)
    }

    fn GetTextExt(
        &self,
        _vcview: u32,
        _acpstart: i32,
        _acpend: i32,
        prc: *mut RECT,
        pfclipped: *mut BOOL,
    ) -> Result<()> {
        unsafe {
            if !prc.is_null() {
                *prc = RECT::default();
            }
            if !pfclipped.is_null() {
                *pfclipped = FALSE;
            }
        }
        Ok(())
    }

    fn GetScreenExt(&self, _vcview: u32) -> Result<RECT> {
        Ok(RECT::default())
    }

    fn GetWnd(&self, _vcview: u32) -> Result<HWND> {
        Ok(self.hwnd)
    }
}
