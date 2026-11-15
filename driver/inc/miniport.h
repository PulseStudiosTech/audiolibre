#pragma once
#include "driver.h"

// IMiniportWaveRT implementation for AudioLibre virtual audio device
class CMiniportWaveRT :
    public IMiniportWaveRT,
    public CUnknown
{
public:
    DECLARE_STD_UNKNOWN();
    DEFINE_STD_CONSTRUCTOR(CMiniportWaveRT);
    ~CMiniportWaveRT();

    // IMiniportWaveRT
    STDMETHODIMP_(NTSTATUS) Init(
        _In_ PUNKNOWN        UnknownAdapter,
        _In_ PRESOURCELIST   ResourceList,
        _In_ PPORTWAVERT     Port
    );
    STDMETHODIMP_(NTSTATUS) NewStream(
        _Out_ PMINIPORTWAVERTSTREAM* Stream,
        _In_  PPORTWAVERTSTREAM      PortStream,
        _In_  ULONG                  Pin,
        _In_  BOOLEAN                Capture,
        _In_  PKSDATAFORMAT          DataFormat
    );
    STDMETHODIMP_(NTSTATUS) GetDeviceDescription(
        _Out_ PDEVICE_DESCRIPTION DeviceDescription
    );

    // Helpers
    NTSTATUS ValidateFormat(_In_ PKSDATAFORMAT DataFormat);

private:
    PPORTWAVERT m_Port;
    PPCFILTER_DESCRIPTOR m_FilterDesc;
    LONG  m_Volume;   // 0–0x10000 fixed-point
    ULONG m_DeviceIndex; // 0 = Stream Mix, 1 = Headphones Mix
};

// IMiniportWaveRTStream for a single open stream
class CMiniportWaveRTStream :
    public IMiniportWaveRTStream,
    public CUnknown
{
public:
    DECLARE_STD_UNKNOWN();
    DEFINE_STD_CONSTRUCTOR(CMiniportWaveRTStream);
    ~CMiniportWaveRTStream();

    STDMETHODIMP_(NTSTATUS) AllocateAudioBuffer(
        _In_  ULONG                RequestedSize,
        _Out_ PMDL*                AudioBufferMdl,
        _Out_ ULONG*               ActualSize,
        _Out_ ULONG*               OffsetFromFirstPage,
        _Out_ MEMORY_CACHING_TYPE* CacheType
    );
    STDMETHODIMP_(void) FreeAudioBuffer(
        _In_opt_ PMDL   AudioBufferMdl,
        _In_     ULONG  BufferSize
    );
    STDMETHODIMP_(void) GetHWLatency(_Out_ KSRTAUDIO_HWLATENCY* hwLatency);
    STDMETHODIMP_(NTSTATUS) GetPosition(_Out_ KSAUDIO_POSITION* Position);
    STDMETHODIMP_(NTSTATUS) GetClockRegister(_Out_ KSRTAUDIO_HWLATENCY* Register);
    STDMETHODIMP_(NTSTATUS) SetFormat(_In_ PKSDATAFORMAT DataFormat);
    STDMETHODIMP_(NTSTATUS) SetState(_In_ KSSTATE State);

private:
    BOOL   m_Capture;
    KSSTATE m_State;
    PMDL    m_Mdl;
    PVOID   m_Buffer;
    ULONG   m_BufferSize;
    ULONG   m_WritePosition;
    ULONG   m_ReadPosition;
    LARGE_INTEGER m_StartTime;
    LONG    m_Volume;
    ULONG   m_SampleRate;
};

// Factory functions
NTSTATUS CreateMiniportWaveRT(
    _Out_ PUNKNOWN* Unknown,
    _In_  REFCLSID,
    _In_opt_ PUNKNOWN UnknownOuter,
    _In_  POOL_FLAGS PoolFlags,
    _In_  ULONG DeviceIndex
);
