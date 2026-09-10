#define NOMINMAX

#include <cstdint>

#include "InternalsPlugin.hpp"
#include "chat_bridge_protocol.h"

#include <windows.h>

#include <algorithm>
#include <cstring>

namespace {

using blackrack_chat_bridge::Memory;
using blackrack_chat_bridge::Message;

class ChatTransport {
public:
    ChatTransport() {
        mapping_ = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            nullptr,
            PAGE_READWRITE,
            0,
            static_cast<DWORD>(sizeof(Memory)),
            blackrack_chat_bridge::kMappingName);
        if (!mapping_) {
            return;
        }
        const bool new_mapping = GetLastError() != ERROR_ALREADY_EXISTS;

        memory_ = static_cast<Memory *>(MapViewOfFile(
            mapping_, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(Memory)));
        if (!memory_) {
            CloseHandle(mapping_);
            mapping_ = nullptr;
            return;
        }

        if (new_mapping || memory_->magic != blackrack_chat_bridge::kMagic ||
            memory_->version != blackrack_chat_bridge::kVersion ||
            memory_->capacity != blackrack_chat_bridge::kCapacity ||
            memory_->text_bytes != blackrack_chat_bridge::kTextBytes) {
            reset_mapping();
        }

        event_ = CreateEventW(
            nullptr,
            FALSE,
            FALSE,
            blackrack_chat_bridge::kEventName);
    }

    ~ChatTransport() {
        set_active(false);
        if (memory_) {
            UnmapViewOfFile(memory_);
        }
        if (event_) {
            CloseHandle(event_);
        }
        if (mapping_) {
            CloseHandle(mapping_);
        }
    }

    void set_active(bool active) {
        if (!memory_) {
            return;
        }
        InterlockedExchange(&memory_->active, active ? 1L : 0L);
        memory_->owner_pid = active ? GetCurrentProcessId() : 0;
    }

    void reset_for_startup() {
        if (!memory_) {
            return;
        }
        InterlockedExchange(&memory_->read_sequence, 0L);
        InterlockedExchange(&memory_->write_sequence, 0L);
        set_active(true);
    }

    void publish(const MessageInfoV01 &info) {
        if (!memory_ || InterlockedCompareExchange(&memory_->active, 0L, 0L) == 0L) {
            return;
        }

        const long write_sequence =
            InterlockedCompareExchange(&memory_->write_sequence, 0L, 0L);
        long read_sequence =
            InterlockedCompareExchange(&memory_->read_sequence, 0L, 0L);
        if (write_sequence - read_sequence >=
            static_cast<long>(blackrack_chat_bridge::kCapacity)) {
            ++read_sequence;
            InterlockedExchange(&memory_->read_sequence, read_sequence);
        }

        Message &message = memory_->messages[
            static_cast<unsigned long>(write_sequence) % blackrack_chat_bridge::kCapacity];
        std::memset(&message, 0, sizeof(message));
        message.sequence = write_sequence;
        message.destination = info.mDestination;
        const std::size_t length = strnlen_s(info.mText, sizeof(info.mText));
        std::memcpy(message.text, info.mText, std::min(length, sizeof(message.text) - 1));
        MemoryBarrier();
        InterlockedExchange(&memory_->write_sequence, write_sequence + 1L);
        if (event_) {
            SetEvent(event_);
        }
    }

private:
    void reset_mapping() {
        std::memset(memory_, 0, sizeof(*memory_));
        memory_->magic = blackrack_chat_bridge::kMagic;
        memory_->version = blackrack_chat_bridge::kVersion;
        memory_->capacity = blackrack_chat_bridge::kCapacity;
        memory_->text_bytes = blackrack_chat_bridge::kTextBytes;
    }

    HANDLE mapping_ = nullptr;
    HANDLE event_ = nullptr;
    Memory *memory_ = nullptr;
};

class BlackRackChatPlugin final : public InternalsPluginV08 {
public:
    void Startup(long) override {
        transport_.reset_for_startup();
    }

    void Shutdown() override {
        transport_.set_active(false);
    }

    bool WantsToDisplayMessage(MessageInfoV01 &info) override {
        // Destination 1 is LMU's chat channel. Returning false preserves the
        // game's own message display while the copy is sent to the overlay.
        if (info.mDestination == 1) {
            transport_.publish(info);
        }
        return false;
    }

private:
    ChatTransport transport_;
};

} // namespace

extern "C" __declspec(dllexport) const char *__cdecl GetPluginName() {
    return "BlackRackChatBridge";
}

extern "C" __declspec(dllexport) PluginObjectType __cdecl GetPluginType() {
    return PO_INTERNALS;
}

extern "C" __declspec(dllexport) int __cdecl GetPluginVersion() {
    return 8;
}

extern "C" __declspec(dllexport) PluginObject *__cdecl CreatePluginObject() {
    return new BlackRackChatPlugin();
}

extern "C" __declspec(dllexport) void __cdecl DestroyPluginObject(PluginObject *object) {
    delete object;
}
