// driver.cpp — DriverEntry and PnP AddDevice for AudioLibre virtual audio driver
#include <portcls.h>
#include <stdunk.h>
#include "../inc/driver.h"
#include "../inc/miniport.h"

// Forward declarations
NTSTATUS AudioLibreAddDevice(_In_ PDRIVER_OBJECT DriverObject, _In_ PDEVICE_OBJECT PhysicalDeviceObject);
NTSTATUS AudioLibreStartDevice(_In_ PDEVICE_OBJECT DeviceObject, _In_ PIRP Irp, _In_ PRESOURCELIST ResourceList);

// ─── DriverEntry ─────────────────────────────────────────────────────────────
EXTERN_C NTSTATUS DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    NTSTATUS ntStatus;

    // Let PortCls do the heavy lifting for IRP dispatch
    ntStatus = PcInitializeAdapterDriver(
        DriverObject,
        RegistryPath,
        (PDRIVER_ADD_DEVICE)AudioLibreAddDevice
    );

    return ntStatus;
}

// ─── AddDevice ───────────────────────────────────────────────────────────────
NTSTATUS AudioLibreAddDevice(
    _In_ PDRIVER_OBJECT DriverObject,
    _In_ PDEVICE_OBJECT PhysicalDeviceObject
)
{
    return PcAddAdapterDevice(
        DriverObject,
        PhysicalDeviceObject,
        PCPFNSTARTDEVICE(AudioLibreStartDevice),
        MAX_MINIPORTS,         // MaxObjects (defined per-device)
        0                      // DeviceExtensionSize (we use PortCls's)
    );
}

// ─── StartDevice ─────────────────────────────────────────────────────────────
// Called when PnP starts the device. We register our miniports with PortCls.
NTSTATUS AudioLibreStartDevice(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp,
    _In_ PRESOURCELIST  ResourceList
)
{
    NTSTATUS ntStatus = STATUS_SUCCESS;
    PUNKNOWN unknownMiniport = NULL;
    PUNKNOWN unknownPort = NULL;

    // ── Stream Mix virtual render device ─────────────────────────────────────
    {
        PPORTWAVERT portWaveRT = NULL;

        ntStatus = PcNewPort(&unknownPort, CLSID_PortWaveRT);
        if (!NT_SUCCESS(ntStatus)) goto cleanup;

        ntStatus = unknownPort->QueryInterface(IID_IPortWaveRT, (PVOID*)&portWaveRT);
        if (!NT_SUCCESS(ntStatus)) goto cleanup;

        ntStatus = CreateMiniportWaveRT(&unknownMiniport, CLSID_AudioLibreStreamMix, NULL,
                                        NonPagedPoolNx, 0 /* Stream Mix index */);
        if (!NT_SUCCESS(ntStatus)) { portWaveRT->Release(); goto cleanup; }

        PUNKNOWN unknownMiniportLocal = unknownMiniport;

        ntStatus = PcRegisterSubdevice(DeviceObject,
                                       AUDIOLIBRE_STREAM_NAME,
                                       unknownMiniportLocal);
        if (!NT_SUCCESS(ntStatus)) { portWaveRT->Release(); goto cleanup; }

        ntStatus = portWaveRT->Init(DeviceObject, Irp, unknownMiniportLocal, ResourceList, NULL);
        portWaveRT->Release();
        if (!NT_SUCCESS(ntStatus)) goto cleanup;

        unknownMiniport->Release();
        unknownMiniport = NULL;
        unknownPort->Release();
        unknownPort = NULL;
    }

    // ── Headphones Mix virtual render device ─────────────────────────────────
    {
        PPORTWAVERT portWaveRT = NULL;

        ntStatus = PcNewPort(&unknownPort, CLSID_PortWaveRT);
        if (!NT_SUCCESS(ntStatus)) goto cleanup;

        ntStatus = unknownPort->QueryInterface(IID_IPortWaveRT, (PVOID*)&portWaveRT);
        if (!NT_SUCCESS(ntStatus)) goto cleanup;

        ntStatus = CreateMiniportWaveRT(&unknownMiniport, CLSID_AudioLibreHeadphonesMix, NULL,
                                        NonPagedPoolNx, 1 /* Headphones Mix index */);
        if (!NT_SUCCESS(ntStatus)) { portWaveRT->Release(); goto cleanup; }

        ntStatus = PcRegisterSubdevice(DeviceObject,
                                       AUDIOLIBRE_PHONES_NAME,
                                       unknownMiniport);
        if (!NT_SUCCESS(ntStatus)) { portWaveRT->Release(); goto cleanup; }

        ntStatus = portWaveRT->Init(DeviceObject, Irp, unknownMiniport, ResourceList, NULL);
        portWaveRT->Release();
        if (!NT_SUCCESS(ntStatus)) goto cleanup;
    }

cleanup:
    if (unknownMiniport) unknownMiniport->Release();
    if (unknownPort)     unknownPort->Release();
    return ntStatus;
}

// Number of subdevices (ports) we register
#define MAX_MINIPORTS 2
