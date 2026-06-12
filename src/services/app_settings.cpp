#include "app_settings.h"
#include "storage_paths.h"
#include "../../third_party/json.hpp"
#include <fstream>

using json = nlohmann::json;

static std::wstring GetSettingsPath()
{
    return GetClipEverythingSettingsPath();
}

static std::string WToU8(const std::wstring& ws)
{
    if (ws.empty()) return {};
    int n = WideCharToMultiByte(CP_UTF8, 0, ws.c_str(), -1, nullptr, 0, nullptr, nullptr);
    std::string s(n - 1, 0);
    WideCharToMultiByte(CP_UTF8, 0, ws.c_str(), -1, s.data(), n, nullptr, nullptr);
    return s;
}

static std::wstring U8ToW(const std::string& s)
{
    if (s.empty()) return {};
    int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, nullptr, 0);
    std::wstring ws(n - 1, 0);
    MultiByteToWideChar(CP_UTF8, 0, s.c_str(), -1, ws.data(), n);
    return ws;
}

AppSettings AppSettings::Load()
{
    AppSettings s;
    try {
        std::ifstream f(GetSettingsPath());
        if (!f.is_open()) return s;
        json j; f >> j;
        s.showToastNotifications = j.value("showToastNotifications", true);
        s.runAtStartup           = j.value("runAtStartup",           false);
        s.copyMods   = j.value("copyMods",   (UINT)(MOD_WIN | MOD_CONTROL | MOD_NOREPEAT));
        s.copyVk     = j.value("copyVk",     (UINT)'C');
        s.pasteMods  = j.value("pasteMods",  (UINT)(MOD_WIN | MOD_CONTROL | MOD_NOREPEAT));
        s.pasteVk    = j.value("pasteVk",    (UINT)'V');
        s.copyLabel  = U8ToW(j.value("copyLabel",  "Win+Ctrl+C"));
        s.pasteLabel = U8ToW(j.value("pasteLabel", "Win+Ctrl+V"));
        s.overlayBoundsSaved = j.value("overlayBoundsSaved", false);
        s.overlayX           = j.value("overlayX", 0);
        s.overlayY           = j.value("overlayY", 0);
        s.overlayHeight      = j.value("overlayHeight", 0);
    } catch (...) {}
    return s;
}

void AppSettings::Save() const
{
    try {
        json j;
        j["showToastNotifications"] = showToastNotifications;
        j["runAtStartup"]           = runAtStartup;
        j["copyMods"]               = copyMods;
        j["copyVk"]                 = copyVk;
        j["pasteMods"]              = pasteMods;
        j["pasteVk"]                = pasteVk;
        j["copyLabel"]              = WToU8(copyLabel);
        j["pasteLabel"]             = WToU8(pasteLabel);
        j["overlayBoundsSaved"]     = overlayBoundsSaved;
        j["overlayX"]               = overlayX;
        j["overlayY"]               = overlayY;
        j["overlayHeight"]          = overlayHeight;

        std::ofstream f(GetSettingsPath());
        f << j.dump(2);
    } catch (...) {}
}
