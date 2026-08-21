#pragma once
#include <portcls.h>
#include <stdunk.h>
#include <ksdebug.h>

// Guids for our virtual audio devices
// {A1B2C3D4-E5F6-7890-ABCD-EF1234567801}
DEFINE_GUID(CLSID_AudioLibreStreamMix,
    0xa1b2c3d4, 0xe5f6, 0x7890, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x01);

// {A1B2C3D4-E5F6-7890-ABCD-EF1234567802}
DEFINE_GUID(CLSID_AudioLibreHeadphonesMix,
    0xa1b2c3d4, 0xe5f6, 0x7890, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x02);

// Device names
#define AUDIOLIBRE_MFG_NAME     L"Pulse Studios"
#define AUDIOLIBRE_STREAM_NAME  L"AudioLibre Stream Mix"
#define AUDIOLIBRE_PHONES_NAME  L"AudioLibre Headphones Mix"
#define AUDIOLIBRE_SYMBOLIC     L"\\DosDevices\\AudioLibre"

// Number of virtual devices exposed
#define AUDIOLIBRE_DEVICE_COUNT 2

// Shared IOCTL interface with userspace daemon
#define AUDIOLIBRE_IOCTL_BASE   0x8300
#define IOCTL_AL_GET_POSITION   CTL_CODE(AUDIOLIBRE_IOCTL_BASE, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_AL_SET_VOLUME     CTL_CODE(AUDIOLIBRE_IOCTL_BASE, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_AL_GET_BUFFER_PTR CTL_CODE(AUDIOLIBRE_IOCTL_BASE, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)

// Supported sample formats
#define AUDIOLIBRE_SAMPLE_RATE    48000
#define AUDIOLIBRE_CHANNELS       2
#define AUDIOLIBRE_BITS_PER_SAMPLE 32
#define AUDIOLIBRE_BLOCK_ALIGN    (AUDIOLIBRE_CHANNELS * AUDIOLIBRE_BITS_PER_SAMPLE / 8)
#define AUDIOLIBRE_BYTE_RATE      (AUDIOLIBRE_SAMPLE_RATE * AUDIOLIBRE_BLOCK_ALIGN)

// Buffer size: 100ms at 48kHz stereo 32-bit
#define AUDIOLIBRE_BUFFER_SIZE    (AUDIOLIBRE_BYTE_RATE / 10)

EXTERN_C DRIVER_INITIALIZE DriverEntry;
EXTERN_C _Dispatch_type_(IRP_MJ_PNP) DRIVER_DISPATCH AddDevice;
