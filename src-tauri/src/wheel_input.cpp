#define NOMINMAX
#define DIRECTINPUT_VERSION 0x0800
#include <windows.h>
#include <dinput.h>
#include <mmsystem.h>

#include <array>
#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <vector>

#pragma comment(lib, "dinput8.lib")
#pragma comment(lib, "dxguid.lib")
#pragma comment(lib, "winmm.lib")

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

struct WinmmDevice {
  UINT id = 0;
  WORD manufacturer_id = 0;
  WORD product_id = 0;
  DWORD buttons = 0;
  DWORD button_count = 0;
  std::array<std::uint16_t, 128> name{};
  bool initialized = false;
};

IDirectInput8W *direct_input = nullptr;
HWND cooperative_window = nullptr;
std::vector<Device> devices;
std::vector<WinmmDevice> winmm_devices;
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

  const auto count = joyGetNumDevs();
  for (UINT id = 0; id < count; ++id) {
    if (std::any_of(winmm_devices.begin(), winmm_devices.end(),
                    [id](const WinmmDevice &device) { return device.id == id; })) {
      continue;
    }
    JOYCAPSW capabilities{};
    if (joyGetDevCapsW(id, &capabilities, sizeof(capabilities)) != JOYERR_NOERROR) {
      continue;
    }
    WinmmDevice device;
    device.id = id;
    device.manufacturer_id = capabilities.wMid;
    device.product_id = capabilities.wPid;
    device.button_count = std::min<DWORD>(capabilities.wNumButtons, 32);
    for (std::size_t index = 0; index + 1 < device.name.size() &&
                                capabilities.szPname[index] != L'\0';
         ++index) {
      device.name[index] = static_cast<std::uint16_t>(capabilities.szPname[index]);
    }
    winmm_devices.push_back(std::move(device));
  }
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

void winmm_device_id(const WinmmDevice &device, std::uint8_t (&id)[16]) {
  std::fill(std::begin(id), std::end(id), 0);
  id[0] = 'W';
  id[1] = 'M';
  id[2] = 'M';
  id[4] = static_cast<std::uint8_t>(device.id & 0xff);
  id[8] = static_cast<std::uint8_t>(device.manufacturer_id & 0xff);
  id[9] = static_cast<std::uint8_t>(device.manufacturer_id >> 8);
  id[10] = static_cast<std::uint8_t>(device.product_id & 0xff);
  id[11] = static_cast<std::uint8_t>(device.product_id >> 8);
}

bool poll_winmm(WheelButtonEvent *event) {
  bool found = false;
  for (auto &device : winmm_devices) {
    JOYINFOEX state{};
    state.dwSize = sizeof(state);
    state.dwFlags = JOY_RETURNBUTTONS;
    if (joyGetPosEx(device.id, &state) != JOYERR_NOERROR) {
      device.initialized = false;
      continue;
    }
    if (!device.initialized) {
      device.buttons = state.dwButtons;
      device.initialized = true;
      continue;
    }
    const DWORD pressed_edges = state.dwButtons & ~device.buttons;
    device.buttons = state.dwButtons;
    if (found || pressed_edges == 0) continue;
    for (std::uint16_t button = 0; button < device.button_count; ++button) {
      if ((pressed_edges & (DWORD{1} << button)) == 0) continue;
      winmm_device_id(device, event->device_id);
      std::copy(device.name.begin(), device.name.end(), event->device_name);
      event->button = button;
      found = true;
      break;
    }
  }
  return found;
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

  WheelButtonEvent direct_input_event{};
  bool direct_input_found = false;
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
      if (!direct_input_found && pressed && !was_pressed) {
        std::memcpy(direct_input_event.device_id, &device.instance_id,
                    sizeof(device.instance_id));
        std::copy(device.name.begin(), device.name.end(),
                  direct_input_event.device_name);
        direct_input_event.button = button;
        direct_input_found = true;
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
  WheelButtonEvent winmm_event{};
  const bool winmm_found = poll_winmm(&winmm_event);
  if (winmm_found) {
    *event = winmm_event;
    return true;
  }
  if (direct_input_found) {
    *event = direct_input_event;
    return true;
  }
  return false;
}

extern "C" void wheel_input_shutdown() {
  for (auto &device : devices) {
    if (device.input) {
      device.input->Unacquire();
      device.input->Release();
    }
  }
  devices.clear();
  winmm_devices.clear();
  if (direct_input) direct_input->Release();
  direct_input = nullptr;
}
