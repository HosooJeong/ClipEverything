#pragma once
#include "models.h"
#include "../core/source_detector.h"
#include "../../third_party/sqlite3.h"
#include <vector>
#include <string>
#include <optional>

class Repository {
public:
    Repository();
    ~Repository();

    bool Initialize();  // %APPDATA%\ClipEverything\clips.db 열기 + 스키마 생성

    // WRITE
    int64_t SaveOrUpdate(const ClipboardSnapshot& snap, const SourceInfo& src);

    // READ
    std::vector<ClipboardItem>   GetItems(const std::wstring& search = {},
                                          const std::wstring& app    = {});
    std::vector<ClipboardFormat> GetFormats(int64_t itemId);

    // UPDATE
    void Rename(int64_t id, const std::wstring& name);
    void SetFavorite(int64_t id, bool fav);
    void SetTags(int64_t id, const std::wstring& tags);

    // DELETE
    void Delete(int64_t id);
    void DeleteAll();

private:
    sqlite3* _db = nullptr;

    // Prepared statements (재사용)
    sqlite3_stmt* _stmtInsert   = nullptr;
    sqlite3_stmt* _stmtFindHash = nullptr;

    void Execute(const char* sql);
    int64_t FindByHash(const std::string& hash);
    int64_t InsertItem(const ClipboardSnapshot& snap, const SourceInfo& src, const std::string& now);

    // wstring ↔ UTF-8 변환
    static std::string ToUtf8(const std::wstring& ws);
    static std::wstring FromUtf8(const char* s);
};
