// stream.cpp — IMiniportWaveRTStream implementation
// Manages a WaveRT cyclic buffer shared between the kernel driver and the audio engine.
#include <portcls.h>
#include <stdunk.h>
#include "../inc/driver.h"
#include "../inc/miniport.h"

CMiniportWaveRTStream::~CMiniportWaveRTStream()
{
    if (m_Mdl && m_Buffer) {
        MmUnmapLockedPages(m_Buffer, m_Mdl);
        IoFreeMdl(m_Mdl);
        m_Mdl = NULL;
        m_Buffer = NULL;
    }
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRTStream::AllocateAudioBuffer(
    _In_  ULONG                RequestedSize,
    _Out_ PMDL*                AudioBufferMdl,
    _Out_ ULONG*               ActualSize,
    _Out_ ULONG*               OffsetFromFirstPage,
    _Out_ MEMORY_CACHING_TYPE* CacheType
)
{
    // Round up to page size
    ULONG bufSize = ((RequestedSize + PAGE_SIZE - 1) & ~(PAGE_SIZE - 1));
    if (bufSize < AUDIOLIBRE_BUFFER_SIZE) bufSize = AUDIOLIBRE_BUFFER_SIZE;

    PHYSICAL_ADDRESS lowAddr  = { 0, 0 };
    PHYSICAL_ADDRESS highAddr = { 0, 0xFFFFFFFF };
    PHYSICAL_ADDRESS skipBytes = { 0, 0 };

    // Allocate contiguous non-paged memory
    PVOID buffer = MmAllocateContiguousMemorySpecifyCache(
        bufSize, lowAddr, highAddr, skipBytes, MmCached);
    if (!buffer) return STATUS_INSUFFICIENT_RESOURCES;

    RtlZeroMemory(buffer, bufSize);

    // Build MDL for this buffer so user-mode (and the port) can map it
    PMDL mdl = IoAllocateMdl(buffer, bufSize, FALSE, FALSE, NULL);
    if (!mdl) {
        MmFreeContiguousMemory(buffer);
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    MmBuildMdlForNonPagedPool(mdl);

    m_Buffer     = buffer;
    m_Mdl        = mdl;
    m_BufferSize = bufSize;
    m_WritePosition = 0;
    m_ReadPosition  = 0;

    *AudioBufferMdl      = mdl;
    *ActualSize          = bufSize;
    *OffsetFromFirstPage = 0;
    *CacheType           = MmCached;

    KeQuerySystemTime(&m_StartTime);
    return STATUS_SUCCESS;
}

STDMETHODIMP_(void) CMiniportWaveRTStream::FreeAudioBuffer(
    _In_opt_ PMDL  AudioBufferMdl,
    _In_     ULONG BufferSize
)
{
    UNREFERENCED_PARAMETER(BufferSize);
    if (AudioBufferMdl && m_Buffer) {
        MmFreeContiguousMemory(m_Buffer);
        IoFreeMdl(AudioBufferMdl);
        m_Buffer = NULL;
        m_Mdl    = NULL;
    }
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRTStream::GetPosition(
    _Out_ KSAUDIO_POSITION* Position
)
{
    if (!Position) return STATUS_INVALID_PARAMETER;

    // Compute elapsed bytes based on wall clock (software position tracking)
    LARGE_INTEGER now;
    KeQuerySystemTime(&now);

    LONGLONG elapsed100ns = now.QuadPart - m_StartTime.QuadPart;
    if (m_State != KSSTATE_RUN || elapsed100ns < 0) {
        Position->PlayOffset  = m_ReadPosition;
        Position->WriteOffset = m_WritePosition;
        return STATUS_SUCCESS;
    }

    // 100ns units to bytes
    ULONGLONG bytesMoved = (ULONGLONG)elapsed100ns * AUDIOLIBRE_BYTE_RATE / 10000000ULL;
    Position->PlayOffset  = (ULONGLONG)(bytesMoved % m_BufferSize);
    Position->WriteOffset = (Position->PlayOffset + AUDIOLIBRE_BUFFER_SIZE / 2) % m_BufferSize;
    return STATUS_SUCCESS;
}

STDMETHODIMP_(void) CMiniportWaveRTStream::GetHWLatency(
    _Out_ KSRTAUDIO_HWLATENCY* hwLatency
)
{
    ASSERT(hwLatency);
    hwLatency->FifoSize    = 0;
    hwLatency->ChipsetDelay = 0;
    hwLatency->CodecDelay  = 0;
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRTStream::GetClockRegister(
    _Out_ KSRTAUDIO_HWLATENCY* Register
)
{
    UNREFERENCED_PARAMETER(Register);
    // We don't expose a hardware clock register — use software position only
    return STATUS_NOT_IMPLEMENTED;
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRTStream::SetFormat(
    _In_ PKSDATAFORMAT DataFormat
)
{
    UNREFERENCED_PARAMETER(DataFormat);
    return STATUS_SUCCESS;
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRTStream::SetState(
    _In_ KSSTATE State
)
{
    if (State == KSSTATE_RUN && m_State != KSSTATE_RUN) {
        KeQuerySystemTime(&m_StartTime);
    }
    m_State = State;
    return STATUS_SUCCESS;
}
