// topology.cpp — Audio filter topology for AudioLibre virtual devices
// Defines the pin/node/connection graph that PortCls presents to Windows audio.
#include <portcls.h>
#include <ksmedia.h>
#include "../inc/driver.h"

// ─── Data ranges ────────────────────────────────────────────────────────────

static KSDATARANGE_AUDIO PinDataRangesBridge[] = {
    {
        sizeof(KSDATARANGE),
        0,
        0,
        0,
        STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
        STATICGUIDOF(KSDATAFORMAT_SUBTYPE_ANALOG),
        STATICGUIDOF(KSDATAFORMAT_SPECIFIER_NONE)
    }
};

static PKSDATARANGE PinDataRangePointersBridge[] = {
    (PKSDATARANGE)&PinDataRangesBridge[0]
};

// PCM float 32-bit stereo @ 48kHz
static KSDATARANGE_AUDIO PinDataRangesStream[] = {
    {
        {
            sizeof(KSDATARANGE_AUDIO),
            KSDATARANGE_ATTRIBUTES,
            0,
            0,
            STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
            STATICGUIDOF(KSDATAFORMAT_SUBTYPE_IEEE_FLOAT),
            STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)
        },
        AUDIOLIBRE_CHANNELS,       // max channels
        AUDIOLIBRE_CHANNELS,       // min channels
        AUDIOLIBRE_BITS_PER_SAMPLE,// max bits/sample
        AUDIOLIBRE_BITS_PER_SAMPLE,// min bits/sample
        AUDIOLIBRE_SAMPLE_RATE,    // max sample rate
        AUDIOLIBRE_SAMPLE_RATE     // min sample rate
    }
};

static PKSDATARANGE PinDataRangePointersStream[] = {
    (PKSDATARANGE)&PinDataRangesStream[0]
};

// ─── Pins ───────────────────────────────────────────────────────────────────

// Pin indices
enum {
    PIN_RENDER_SINK = 0,     // Render sink (host → driver, i.e. apps write here)
    PIN_RENDER_BRIDGE,       // Bridge to "hardware"
    PIN_CAPTURE_SOURCE,      // Capture source (driver → host, loopback read)
    PIN_CAPTURE_BRIDGE,      // Bridge from "hardware"
    PIN_COUNT
};

static PCPIN_DESCRIPTOR MiniportPins[] = {
    // [0] Render sink — SINK from host (apps render into this)
    {
        AUDIOLIBRE_BUFFER_SIZE / AUDIOLIBRE_BLOCK_ALIGN, // MaxGlobalInstanceCount
        AUDIOLIBRE_BUFFER_SIZE / AUDIOLIBRE_BLOCK_ALIGN, // MaxFilterInstanceCount
        0,                                               // MinFilterInstanceCount
        NULL,                                            // AutomationTable
        {
            0,
            NULL,
            0,
            NULL,
            SIZEOF_ARRAY(PinDataRangePointersStream),
            PinDataRangePointersStream,
            KSPIN_DATAFLOW_IN,
            KSPIN_COMMUNICATION_SINK,
            &KSCATEGORY_AUDIO,
            NULL,
            0
        }
    },
    // [1] Render bridge — connects to "DAC"
    {
        0, 0, 0, NULL,
        {
            0, NULL, 0, NULL,
            SIZEOF_ARRAY(PinDataRangePointersBridge),
            PinDataRangePointersBridge,
            KSPIN_DATAFLOW_OUT,
            KSPIN_COMMUNICATION_NONE,
            &KSCATEGORY_AUDIO,
            NULL,
            0
        }
    },
    // [2] Capture source — loopback read (host reads back what was rendered)
    {
        AUDIOLIBRE_BUFFER_SIZE / AUDIOLIBRE_BLOCK_ALIGN,
        AUDIOLIBRE_BUFFER_SIZE / AUDIOLIBRE_BLOCK_ALIGN,
        0, NULL,
        {
            0, NULL, 0, NULL,
            SIZEOF_ARRAY(PinDataRangePointersStream),
            PinDataRangePointersStream,
            KSPIN_DATAFLOW_OUT,
            KSPIN_COMMUNICATION_SOURCE,
            &KSCATEGORY_AUDIO,
            NULL,
            0
        }
    },
    // [3] Capture bridge — connects from "ADC"
    {
        0, 0, 0, NULL,
        {
            0, NULL, 0, NULL,
            SIZEOF_ARRAY(PinDataRangePointersBridge),
            PinDataRangePointersBridge,
            KSPIN_DATAFLOW_IN,
            KSPIN_COMMUNICATION_NONE,
            &KSCATEGORY_AUDIO,
            NULL,
            0
        }
    }
};

// ─── Nodes ──────────────────────────────────────────────────────────────────

// Automation tables for volume node
static KSPROPERTY_ITEM VolumePropertyItems[] = {
    DEFINE_KSPROPERTY_ITEM_AUDIO_VOLUMELEVEL(
        NULL, // GetVolumeLevel
        NULL  // SetVolumeLevel
    )
};

static KSPROPERTY_SET VolumePropertySet[] = {
    DEFINE_KSPROPERTY_SET(
        &KSPROPSETID_Audio,
        SIZEOF_ARRAY(VolumePropertyItems),
        VolumePropertyItems,
        0,
        NULL
    )
};

static PCAUTOMATION_TABLE VolumeAutomation = {
    SIZEOF_ARRAY(VolumePropertySet),
    VolumePropertySet,
    0, NULL,
    0, NULL,
    0
};

enum {
    NODE_VOLUME = 0,
    NODE_MUTE,
    NODE_COUNT
};

static PCNODE_DESCRIPTOR MiniportNodes[] = {
    // [0] Volume control
    {
        0,
        &VolumeAutomation,
        &KSNODETYPE_VOLUME,
        NULL
    },
    // [1] Mute
    {
        0,
        NULL,
        &KSNODETYPE_MUTE,
        NULL
    }
};

// ─── Connections ────────────────────────────────────────────────────────────

static PCCONNECTION_DESCRIPTOR MiniportConnections[] = {
    // Render path: PIN_RENDER_SINK → Volume → Mute → PIN_RENDER_BRIDGE
    { PCFILTER_NODE, PIN_RENDER_SINK,   NODE_VOLUME, 1 },
    { NODE_VOLUME,   1,                 NODE_MUTE,   1 },
    { NODE_MUTE,     1,                 PCFILTER_NODE, PIN_RENDER_BRIDGE },
    // Capture loopback: PIN_CAPTURE_BRIDGE → Volume → Mute → PIN_CAPTURE_SOURCE
    { PCFILTER_NODE, PIN_CAPTURE_BRIDGE, NODE_VOLUME, 0 },
    { NODE_VOLUME,   0,                  NODE_MUTE,   0 },
    { NODE_MUTE,     0,                  PCFILTER_NODE, PIN_CAPTURE_SOURCE }
};

// ─── Filter descriptor ──────────────────────────────────────────────────────

PCFILTER_DESCRIPTOR AudioLibreFilterDescriptor = {
    0,                              // Version
    NULL,                           // AutomationTable
    sizeof(PCPIN_DESCRIPTOR),
    SIZEOF_ARRAY(MiniportPins),
    MiniportPins,
    sizeof(PCNODE_DESCRIPTOR),
    SIZEOF_ARRAY(MiniportNodes),
    MiniportNodes,
    sizeof(PCCONNECTION_DESCRIPTOR),
    SIZEOF_ARRAY(MiniportConnections),
    MiniportConnections,
    PCFILTER_NODE               // CategoryCount / Categories (filled by PortCls)
};
