//! ITfDisplayAttributeProvider implementation — provides text display attributes.
//!
//! Defines how preedit text looks (underline for composing, bold for converted).

use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::core::*;

use crate::globals::*;
use crate::tsf::text_input_processor::KarukanTextService_Impl;

#[allow(clippy::not_unsafe_ptr_arg_deref)]
impl ITfDisplayAttributeProvider_Impl for KarukanTextService_Impl {
    /// Return an enumerator of display attributes.
    fn EnumDisplayAttributeInfo(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        let attrs: Vec<ITfDisplayAttributeInfo> = vec![
            KarukanInputAttribute.into(),
            KarukanConvertedAttribute.into(),
        ];
        Ok(KarukanDisplayAttributeEnum::new(attrs).into())
    }

    /// Get a display attribute by GUID.
    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        unsafe {
            if guid.is_null() {
                return Err(E_INVALIDARG.into());
            }
            let guid = *guid;
            if guid == GUID_DISPLAY_ATTRIBUTE_INPUT {
                Ok(KarukanInputAttribute.into())
            } else if guid == GUID_DISPLAY_ATTRIBUTE_CONVERTED {
                Ok(KarukanConvertedAttribute.into())
            } else {
                Err(E_INVALIDARG.into())
            }
        }
    }
}

/// Display attribute for composing (input) text — thin underline.
#[implement(ITfDisplayAttributeInfo)]
struct KarukanInputAttribute;

impl ITfDisplayAttributeInfo_Impl for KarukanInputAttribute_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(GUID_DISPLAY_ATTRIBUTE_INPUT)
    }

    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("karukan Input"))
    }

    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        unsafe {
            if pda.is_null() {
                return Err(E_POINTER.into());
            }
            *pda = TF_DISPLAYATTRIBUTE {
                crText: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    ..Default::default()
                },
                crBk: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    ..Default::default()
                },
                lsStyle: TF_LS_DOT,
                fBoldLine: FALSE,
                crLine: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    ..Default::default()
                },
                bAttr: TF_ATTR_INPUT,
            };
            Ok(())
        }
    }

    fn SetAttributeInfo(&self, _pda: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

/// Display attribute for converted text — bold underline.
#[implement(ITfDisplayAttributeInfo)]
struct KarukanConvertedAttribute;

impl ITfDisplayAttributeInfo_Impl for KarukanConvertedAttribute_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(GUID_DISPLAY_ATTRIBUTE_CONVERTED)
    }

    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("karukan Converted"))
    }

    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        unsafe {
            if pda.is_null() {
                return Err(E_POINTER.into());
            }
            *pda = TF_DISPLAYATTRIBUTE {
                crText: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    ..Default::default()
                },
                crBk: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    ..Default::default()
                },
                lsStyle: TF_LS_SOLID,
                fBoldLine: TRUE,
                crLine: TF_DA_COLOR {
                    r#type: TF_CT_NONE,
                    ..Default::default()
                },
                bAttr: TF_ATTR_TARGET_CONVERTED,
            };
            Ok(())
        }
    }

    fn SetAttributeInfo(&self, _pda: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

/// Enumerator for display attributes.
#[implement(IEnumTfDisplayAttributeInfo)]
struct KarukanDisplayAttributeEnum {
    attrs: Vec<ITfDisplayAttributeInfo>,
    index: std::cell::Cell<usize>,
}

impl KarukanDisplayAttributeEnum {
    fn new(attrs: Vec<ITfDisplayAttributeInfo>) -> Self {
        Self {
            attrs,
            index: std::cell::Cell::new(0),
        }
    }
}

impl IEnumTfDisplayAttributeInfo_Impl for KarukanDisplayAttributeEnum_Impl {
    fn Clone(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        let cloned = KarukanDisplayAttributeEnum {
            attrs: self.attrs.clone(),
            index: self.index.clone(),
        };
        Ok(cloned.into())
    }

    fn Next(
        &self,
        celt: u32,
        rginfo: *mut Option<ITfDisplayAttributeInfo>,
        pcfetched: *mut u32,
    ) -> Result<()> {
        let mut fetched = 0u32;
        let idx = self.index.get();

        for i in 0..celt as usize {
            let pos = idx + i;
            if pos >= self.attrs.len() {
                break;
            }
            unsafe {
                rginfo.add(i).write(Some(self.attrs[pos].clone()));
            }
            fetched += 1;
        }

        self.index.set(idx + fetched as usize);

        unsafe {
            if !pcfetched.is_null() {
                *pcfetched = fetched;
            }
        }

        if fetched == celt {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn Reset(&self) -> Result<()> {
        self.index.set(0);
        Ok(())
    }

    fn Skip(&self, ulcount: u32) -> Result<()> {
        let new_idx = self.index.get() + ulcount as usize;
        self.index.set(new_idx.min(self.attrs.len()));
        Ok(())
    }
}
