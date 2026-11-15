// miniport.cpp — IMiniportWaveRT implementation for AudioLibre
#include <portcls.h>
#include <stdunk.h>
#include <ksmedia.h>
#include "../inc/driver.h"
#include "../inc/miniport.h"

extern PCFILTER_DESCRIPTOR AudioLibreFilterDescriptor;

// ─── CMiniportWaveRT ─────────────────────────────────────────────────────────

CMiniportWaveRT::~CMiniportWaveRT()
{
    if (m_Port) { m_Port->Release(); m_Port = NULL; }
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRT::Init(
    _In_ PUNKNOWN      UnknownAdapter,
    _In_ PRESOURCELIST ResourceList,
    _In_ PPORTWAVERT   Port
)
{
    UNREFERENCED_PARAMETER(UnknownAdapter);
    UNREFERENCED_PARAMETER(ResourceList);

    ASSERT(Port);
    m_Port = Port;
    m_Port->AddRef();
    m_Volume = 0x10000; // 100%
    m_FilterDesc = &AudioLibreFilterDescriptor;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRT::GetDeviceDescription(
    _Out_ PDEVICE_DESCRIPTION DeviceDescription
)
{
    ASSERT(DeviceDescription);
    RtlZeroMemory(DeviceDescription, sizeof(DEVICE_DESCRIPTION));
    DeviceDescription->Version = DEVICE_DESCRIPTION_VERSION2;
    DeviceDescription->Master = TRUE;
    DeviceDescription->DmaWidth = Width32Bits;
    DeviceDescription->MaximumLength = AUDIOLIBRE_BUFFER_SIZE * 4;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRT::NewStream(
    _Out_ PMINIPORTWAVERTSTREAM* Stream,
    _In_  PPORTWAVERTSTREAM      PortStream,
    _In_  ULONG                  Pin,
    _In_  BOOLEAN                Capture,
    _In_  PKSDATAFORMAT          DataFormat
)
{
    UNREFERENCED_PARAMETER(PortStream);
    UNREFERENCED_PARAMETER(Pin);

    NTSTATUS ntStatus = ValidateFormat(DataFormat);
    if (!NT_SUCCESS(ntStatus)) return ntStatus;

    CMiniportWaveRTStream* stream =
        new(NonPagedPoolNx, 'SRLA') CMiniportWaveRTStream(NULL);
    if (!stream) return STATUS_INSUFFICIENT_RESOURCES;

    stream->AddRef();

    ntStatus = stream->NonDelegatingQueryInterface(
        IID_IMiniportWaveRTStream, (PVOID*)Stream);
    stream->Release();

    if (!NT_SUCCESS(ntStatus)) return ntStatus;

    // Store capture/render flag and format in the stream
    // (stream Init is done inline here for simplicity)
    return STATUS_SUCCESS;
}

NTSTATUS CMiniportWaveRT::ValidateFormat(_In_ PKSDATAFORMAT DataFormat)
{
    if (!DataFormat) return STATUS_INVALID_PARAMETER;
    if (DataFormat->FormatSize < sizeof(KSDATAFORMAT)) return STATUS_INVALID_PARAMETER;

    // We only support IEEE float stereo 48kHz
    if (!IsEqualGUIDAligned(DataFormat->MajorFormat, KSDATAFORMAT_TYPE_AUDIO))
        return STATUS_NOT_SUPPORTED;
    if (!IsEqualGUIDAligned(DataFormat->SubFormat, KSDATAFORMAT_SUBTYPE_IEEE_FLOAT))
        return STATUS_NOT_SUPPORTED;

    if (DataFormat->FormatSize >= sizeof(KSDATAFORMAT_WAVEFORMATEX)) {
        PWAVEFORMATEX wfx = (PWAVEFORMATEX)(DataFormat + 1);
        if (wfx->nSamplesPerSec != AUDIOLIBRE_SAMPLE_RATE) return STATUS_NOT_SUPPORTED;
        if (wfx->nChannels      != AUDIOLIBRE_CHANNELS)    return STATUS_NOT_SUPPORTED;
        if (wfx->wBitsPerSample != AUDIOLIBRE_BITS_PER_SAMPLE) return STATUS_NOT_SUPPORTED;
    }

    return STATUS_SUCCESS;
}

NTSTATUS CreateMiniportWaveRT(
    _Out_ PUNKNOWN* Unknown,
    _In_  REFCLSID,
    _In_opt_ PUNKNOWN UnknownOuter,
    _In_  POOL_FLAGS PoolFlags,
    _In_  ULONG DeviceIndex
)
{
    UNREFERENCED_PARAMETER(PoolFlags);

    CMiniportWaveRT* miniport = new(NonPagedPoolNx, 'MPLA') CMiniportWaveRT(UnknownOuter);
    if (!miniport) return STATUS_INSUFFICIENT_RESOURCES;

    miniport->m_DeviceIndex = DeviceIndex;
    miniport->AddRef();

    NTSTATUS ntStatus = miniport->NonDelegatingQueryInterface(
        IID_IMiniportWaveRT, (PVOID*)Unknown);
    miniport->Release();
    return ntStatus;
}
