//! The suite of traits allowing CPAL to abstract over hosts, devices, event loops and stream IDs.

use std::time::Duration;

use crate::{
    AnyError, BuildStreamError, Data, DefaultStreamConfigError, DeviceId, DeviceIdError,
    DeviceNameError, DevicesError, GetPeriodsError, InputCallbackInfo, InputDevices,
    OutputCallbackInfo, OutputDevices, PauseStreamError, PlayStreamError, SampleFormat,
    SizedSample, StreamConfig, StreamError, SupportedStreamConfig, SupportedStreamConfigRange,
    SupportedStreamConfigsError, SyncStreamError,
};

/// A [`Host`] provides access to the available audio devices on the system.
///
/// Each platform may have a number of available hosts depending on the system, each with their own
/// pros and cons.
///
/// For example, WASAPI is the standard audio host API that ships with the Windows operating
/// system. However, due to historical limitations with respect to performance and flexibility,
/// Steinberg created the ASIO API providing better audio device support for pro audio and
/// low-latency applications. As a result, it is common for some devices and device capabilities to
/// only be available via ASIO, while others are only available via WASAPI.
///
/// Another great example is the Linux platform. While the ALSA host API is the lowest-level API
/// available to almost all distributions of Linux, its flexibility is limited as it requires that
/// each process have exclusive access to the devices with which they establish streams. PulseAudio
/// is another popular host API that aims to solve this issue by providing user-space mixing,
/// however it has its own limitations w.r.t. low-latency and high-performance audio applications.
/// JACK is yet another host API that is more suitable to pro-audio applications, however it is
/// less readily available by default in many Linux distributions and is known to be tricky to
/// set up.
///
/// [`Host`]: crate::Host
pub trait HostTrait {
    /// The type used for enumerating available devices by the host.
    type Devices: Iterator<Item = Self::Device>;
    /// The `Device` type yielded by the host.
    type Device: DeviceTrait;

    /// Whether or not the host is available on the system.
    fn is_available() -> bool;

    /// An iterator yielding all [`Device`](DeviceTrait)s currently available to the host on the system.
    ///
    /// Can be empty if the system does not support audio in general.
    fn devices(&self) -> Result<Self::Devices, DevicesError>;

    /// Fetches a [`Device`](DeviceTrait) based on a [`DeviceId`](DeviceId) if available
    ///
    /// Returns `None` if no device matching the id is found
    fn device_by_id(&self, id: &DeviceId) -> Option<Self::Device> {
        self.devices()
            .ok()?
            .find(|device| device.id().ok().as_ref() == Some(id))
    }

    /// The default input audio device on the system.
    ///
    /// Returns `None` if no input device is available.
    fn default_input_device(&self) -> Option<Self::Device>;

    /// The default output audio device on the system.
    ///
    /// Returns `None` if no output device is available.
    fn default_output_device(&self) -> Option<Self::Device>;

    /// An iterator yielding all `Device`s currently available to the system that support one or more
    /// input stream formats.
    ///
    /// Can be empty if the system does not support audio input.
    fn input_devices(&self) -> Result<InputDevices<Self::Devices>, DevicesError> {
        Ok(self.devices()?.filter(DeviceTrait::supports_input))
    }

    /// An iterator yielding all `Device`s currently available to the system that support one or more
    /// output stream formats.
    ///
    /// Can be empty if the system does not support audio output.
    fn output_devices(&self) -> Result<OutputDevices<Self::Devices>, DevicesError> {
        Ok(self.devices()?.filter(DeviceTrait::supports_output))
    }
}

/// A device that is capable of audio input and/or output.
///
/// Please note that `Device`s may become invalid if they get disconnected. Therefore, all the
/// methods that involve a device return a `Result` allowing the user to handle this case.
pub trait DeviceTrait {
    /// The iterator type yielding supported input stream formats.
    type SupportedInputConfigs: Iterator<Item = SupportedStreamConfigRange>;
    /// The iterator type yielding supported output stream formats.
    type SupportedOutputConfigs: Iterator<Item = SupportedStreamConfigRange>;
    /// The stream type created by [`build_input_stream_raw`] and [`build_output_stream_raw`].
    ///
    /// [`build_input_stream_raw`]: Self::build_input_stream_raw
    /// [`build_output_stream_raw`]: Self::build_output_stream_raw
    type Stream: StreamTrait;

    /// The human-readable name of the device.
    fn name(&self) -> Result<String, DeviceNameError>;

    /// The device-id of the device.
    fn id(&self) -> Result<DeviceId, DeviceIdError>;

    /// True if the device supports audio input, otherwise false
    fn supports_input(&self) -> bool {
        self.supported_input_configs()
            .map(|mut iter| iter.next().is_some())
            .unwrap_or(false)
    }

    /// True if the device supports audio output, otherwise false
    fn supports_output(&self) -> bool {
        self.supported_output_configs()
            .map(|mut iter| iter.next().is_some())
            .unwrap_or(false)
    }

    /// An iterator yielding formats that are supported by the backend.
    ///
    /// Can return an error if the device is no longer valid (e.g. it has been disconnected).
    fn supported_input_configs(
        &self,
    ) -> Result<Self::SupportedInputConfigs, SupportedStreamConfigsError>;

    /// An iterator yielding output stream formats that are supported by the device.
    ///
    /// Can return an error if the device is no longer valid (e.g. it has been disconnected).
    fn supported_output_configs(
        &self,
    ) -> Result<Self::SupportedOutputConfigs, SupportedStreamConfigsError>;

    /// The default input stream format for the device.
    fn default_input_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError>;

    /// The default output stream format for the device.
    fn default_output_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError>;

    /// Create an input stream.
    fn build_input_stream<T, D, E>(
        &self,
        config: &StreamConfig,
        mut data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        T: SizedSample,
        D: FnMut(&[T], &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.build_input_stream_raw(
            config,
            T::FORMAT,
            move |data, info| {
                data_callback(
                    data.as_slice()
                        .expect("host supplied incorrect sample type"),
                    info,
                )
            },
            error_callback,
            timeout,
        )
    }

    /// Create an output stream.
    fn build_output_stream<T, D, E>(
        &self,
        config: &StreamConfig,
        mut data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        T: SizedSample,
        D: FnMut(&mut [T], &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        self.build_output_stream_raw(
            config,
            T::FORMAT,
            move |data, info| {
                data_callback(
                    data.as_slice_mut()
                        .expect("host supplied incorrect sample type"),
                    info,
                )
            },
            error_callback,
            timeout,
        )
    }

    /// Create a dynamically typed input stream.
    fn build_input_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static;

    /// Create a dynamically typed output stream.
    fn build_output_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        data_callback: D,
        error_callback: E,
        timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&mut Data, &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static;
}

/// A stream created from [`Device`](DeviceTrait), with methods to control playback.
pub trait StreamTrait {
    /// Run the stream.
    ///
    /// Note: Not all platforms automatically run the stream upon creation, so it is important to
    /// call `play` after creation if it is expected that the stream should run immediately.
    fn play(&self) -> Result<(), PlayStreamError>;

    /// Some devices support pausing the audio stream. This can be useful for saving energy in
    /// moments of silence.
    ///
    /// Note: Not all devices support suspending the stream at the hardware level. This method may
    /// fail in these cases.
    fn pause(&self) -> Result<(), PauseStreamError>;
}

pub struct Captures<'a> {
    pub data: &'a [u8],
    // todo monotonic clocks
}

pub struct Renders<'a> {
    pub data: &'a mut [u8],
    // todo monotonic clocks
}

#[derive(Default, Debug, Clone, Copy)]
pub struct AfterCapture {
    /// Number of frame still available for capture after a capture pass
    /// Consumer can do another pass instead of wait for event to save
    /// a few thread context switches
    ///
    /// None means the current device does not support querying this info
    pub available_next: Option<usize>,
}

pub trait Source {
    fn capture(
        &mut self,
        f: &mut dyn FnMut(Captures<'_>) -> Result<usize, AnyError>,
    ) -> Result<AfterCapture, SyncStreamError>;
}

#[derive(Default, Debug, Clone, Copy)]
pub struct AfterRender {
    /// Number of frame still available for capture after a capture pass
    /// Consumer can do another pass instead of wait for event to save
    /// a few thread context switches
    ///
    /// None means the current device does not support querying this info
    pub available_next: Option<usize>,
}

pub trait Sink {
    fn render(
        &mut self,
        f: &mut dyn FnMut(Renders<'_>) -> Result<usize, AnyError>,
    ) -> Result<AfterRender, SyncStreamError>;
}

pub trait BuildSource {
    type Output: Source;
    fn build_source(
        &mut self,
        cfg: StreamConfig,
        fmt: SampleFormat,
        period: usize,
        ev: EventHandle,
    ) -> Result<Self::Output, BuildStreamError>;
}

pub trait BuildSink {
    type Output: Sink;
    fn build_sink(
        &mut self,
        cfg: StreamConfig,
        fmt: SampleFormat,
        period: usize,
        ev: EventHandle,
    ) -> Result<Self::Output, BuildStreamError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Period {
    pub default: usize,
    pub min: usize,
    pub max: usize,
}

pub trait Periodcity {
    fn get_periods(&self, cfg: &SupportedStreamConfig) -> Result<Period, GetPeriodsError>;
}

pub enum EventHandle {
    #[cfg(windows)]
    WASAPI(windows::Win32::Foundation::HANDLE),
}

impl From<windows::Win32::Foundation::HANDLE> for EventHandle {
    fn from(value: windows::Win32::Foundation::HANDLE) -> Self {
        Self::WASAPI(value)
    }
}

impl EventHandle {
    #[cfg(windows)]
    pub fn inner(&self) -> Option<&windows::Win32::Foundation::HANDLE> {
        match self {
            Self::WASAPI(h) => Some(h),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}
