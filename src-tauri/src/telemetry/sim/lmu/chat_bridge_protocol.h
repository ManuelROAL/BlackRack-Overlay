#pragma once

#include <cstdint>

// This is a private, versioned IPC contract between the LMU plugin and
// BlackRack Overlay. Keep all fields fixed-size: the mapping is shared by
// C++ and Rust processes with no serializer in between.
namespace blackrack_chat_bridge {

constexpr std::uint32_t kMagic = 0x42435242; // "BCRB"
constexpr std::uint32_t kVersion = 1;
constexpr std::uint32_t kCapacity = 64;
constexpr std::uint32_t kTextBytes = 128;

constexpr wchar_t kMappingName[] = L"Local\\BlackRackOverlay_LMUChat_v1";
constexpr wchar_t kEventName[] = L"Local\\BlackRackOverlay_LMUChat_Event_v1";

#pragma pack(push, 4)
struct Message {
    std::int32_t sequence;
    std::uint8_t destination;
    std::uint8_t reserved[3];
    char text[kTextBytes];
};

struct Memory {
    std::uint32_t magic;
    std::uint32_t version;
    volatile long active;
    volatile std::uint32_t owner_pid;
    volatile long write_sequence;
    volatile long read_sequence;
    std::uint32_t capacity;
    std::uint32_t text_bytes;
    Message messages[kCapacity];
};
#pragma pack(pop)

static_assert(sizeof(Message) == 136, "unexpected chat bridge message layout");
static_assert(sizeof(Memory) == 8736, "unexpected chat bridge mapping layout");

} // namespace blackrack_chat_bridge
