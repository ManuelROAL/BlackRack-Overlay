#define NOMINMAX
#define DIRECTINPUT_VERSION 0x0800
#include <windows.h>
#include <dinput.h>

#include <array>
#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <vector>

#pragma comment(lib, "dinput8.lib")
#pragma comment(lib, "dxguid.lib")

extern "C" {
struct WheelButtonEvent {
  std::uint8_t device_id[16];
  std::uint16_t device_name[128];
  std::uint16_t button;
};
}

namespace {
struct Device {
  IDirectInputDevice8W *input = nullptr;
  GUID instance_id{};
  std::array<std::uint8_t, 128> buttons{};
  std::array<std::uint16_t, 128> name{};
  bool initialized = false;
  std::uint16_t failures = 0;
};

IDirectInput8W *direct_input = nullptr;
HWND cooperative_window = nullptr;
std::vector<Device> devices;
auto last_enumeration = std::chrono::steady_clock::time_point{};

bool same_guid(const GUID &left, const GUID &right) {
  return std::memcmp(&left, &right, sizeof(GUID)) == 0;
}

BOOL CALLBACK enumerate_device(const DIDEVICEINSTANCEW *instance, VOID *) {
  for (const auto &device : devices) {
    if (same_guid(device.instance_id, instance->guidInstance)) return DIENUM_CONTINUE;
  }

  IDirectInputDevice8W *input = nullptr;
  if (FAILED(direct_input->CreateDevice(instance->guidInstance, &input, nullptr))) {
    return DIENUM_CONTINUE;
  }
  if (FAILED(input->SetDataFormat(&c_dfDIJoystick2)) ||
      FAILED(input->SetCooperativeLevel(cooperative_window,
                                       DISCL_BACKGROUND | DISCL_NONEXCLUSIVE))) {
    input->Release();
    return DIENUM_CONTINUE;
  }

  Device device;
  device.input = input;
  device.instance_id = instance->guidInstance;
  for (std::size_t index = 0; index + 1 < device.name.size() &&
                              instance->tszInstanceName[index] != L'\0';
       ++index) {
    device.name[index] = static_cast<std::uint16_t>(instance->tszInstanceName[index]);
  }
  input->Acquire();
  devices.push_back(std::move(device));
  return DIENUM_CONTINUE;
}

void enumerate_devices() {
  const auto now = std::chrono::steady_clock::now();
  if (now - last_enumeration < std::chrono::seconds(2)) return;
  last_enumeration = now;
  direct_input->EnumDevices(DI8DEVCLASS_GAMECTRL, enumerate_device, nullptr,
                            DIEDFL_ATTACHEDONLY);
}

bool read_state(Device &device, DIJOYSTATE2 &state) {
  auto result = device.input->Poll();
  if (FAILED(result)) {
    result = device.input->Acquire();
    while (result == DIERR_INPUTLOST) result = device.input->Acquire();
    if (FAILED(result) || FAILED(device.input->Poll())) return false;
  }
  return SUCCEEDED(device.input->GetDeviceState(sizeof(state), &state));
}
} // namespace

extern "C" bool wheel_input_initialize(std::intptr_t hwnd) {
  cooperative_window = reinterpret_cast<HWND>(hwnd);
  if (!cooperative_window) return false;
  const auto instance = GetModuleHandleW(nullptr);
  if (FAILED(DirectInput8Create(instance, DIRECTINPUT_VERSION, IID_IDirectInput8W,
                                reinterpret_cast<void **>(&direct_input), nullptr))) {
    direct_input = nullptr;
    return false;
  }
  enumerate_devices();
  return true;
}

extern "C" bool wheel_input_poll(WheelButtonEvent *event) {
  if (!direct_input || !event) return false;
  enumerate_devices();

  bool found = false;
  for (auto &device : devices) {
    DIJOYSTATE2 state{};
    if (!read_state(device, state)) {
      device.initialized = false;
      ++device.failures;
      continue;
    }
    device.failures = 0;
    if (!device.initialized) {
      std::copy(std::begin(state.rgbButtons), std::end(state.rgbButtons),
                device.buttons.begin());
      device.initialized = true;
      continue;
    }
    for (std::uint16_t button = 0; button < device.buttons.size(); ++button) {
      const bool pressed = (state.rgbButtons[button] & 0x80) != 0;
      const bool was_pressed = (device.buttons[button] & 0x80) != 0;
      if (!found && pressed && !was_pressed) {
        std::memcpy(event->device_id, &device.instance_id, sizeof(device.instance_id));
        std::copy(device.name.begin(), device.name.end(), event->device_name);
        event->button = button;
        found = true;
      }
      device.buttons[button] = state.rgbButtons[button];
    }
  }
  devices.erase(std::remove_if(devices.begin(), devices.end(), [](Device &device) {
                  if (device.failures < 200) return false;
                  device.input->Unacquire();
                  device.input->Release();
                  return true;
                }),
                devices.end());
  return found;
}

extern "C" void wheel_input_shutdown() {
  for (auto &device : devices) {
    if (device.input) {
      device.input->Unacquire();
      device.input->Release();
    }
  }
  devices.clear();
  if (direct_input) direct_input->Release();
  direct_input = nullptr;
}
