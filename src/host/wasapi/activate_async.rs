use windows::{
    core::{implement, IUnknown, Interface, HRESULT},
    Win32::{
        Foundation::{self, CloseHandle},
        Media::Audio::{
            ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
            IActivateAudioInterfaceCompletionHandler,
            IActivateAudioInterfaceCompletionHandler_Impl, IAudioClient,
            AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        },
        System::{
            Com::StructuredStorage::PROPVARIANT,
            Threading::{CreateEventW, SetEvent, WaitForSingleObject, INFINITE},
            Variant::VT_BLOB,
        },
    },
};

pub fn capture_process(pid: u32, capture_tree: bool) -> Result<IAudioClient, windows::core::Error> {
    use std::mem;
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS::default();
    params.ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK;
    if capture_tree {
        params.Anonymous.ProcessLoopbackParams.ProcessLoopbackMode =
            PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE;
    }
    params.Anonymous.ProcessLoopbackParams.TargetProcessId = pid;
    let mut pv = PROPVARIANT::default();

    unsafe {
        (*pv.Anonymous.Anonymous).vt = VT_BLOB;
        (*pv.Anonymous.Anonymous).Anonymous.blob.cbSize =
            mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32;
        (*pv.Anonymous.Anonymous).Anonymous.blob.pBlobData = &params as *const _ as *mut u8;

        let aud_client: IAudioClient =
            activate_audio_interface_sync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, Some(&pv))?;
        mem::forget(pv);
        Ok(aud_client)
    }
}

pub unsafe fn activate_audio_interface_sync<P0, Out>(
    deviceinterfacepath: P0,
    activationparams: ::core::option::Option<*const PROPVARIANT>,
) -> Result<Out, windows::core::Error>
where
    P0: ::windows::core::Param<::windows::core::PCWSTR>,
    Out: Interface,
{
    unsafe {
        let ev = CreateEventW(None, false, false, None)?;
        let completionhandler: IActivateAudioInterfaceCompletionHandler = SyncHandler(ev).into();
        let result = ActivateAudioInterfaceAsync(
            deviceinterfacepath,
            &Out::IID,
            activationparams,
            &completionhandler,
        )?;

        WaitForSingleObject(ev, INFINITE);

        let mut hr = HRESULT(0);
        let mut ai: Option<IUnknown> = None;
        result.GetActivateResult(&mut hr, &mut ai)?;
        CloseHandle(ev)?;

        if let Some(comi) = ai {
            Ok(comi.cast()?)
        } else {
            let err = windows::core::Error::from(hr);
            Err(err.into())
        }
    }
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct SyncHandler(Foundation::HANDLE);

impl IActivateAudioInterfaceCompletionHandler_Impl for SyncHandler_Impl {
    fn ActivateCompleted(
        &self,
        _: windows::core::Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        unsafe { SetEvent(self.0) }
    }
}
