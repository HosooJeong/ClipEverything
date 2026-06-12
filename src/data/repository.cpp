#include "repository.h"
#include "../core/source_detector.h"
#include "../services/storage_paths.h"
#include <windows.h>
#include <ctime>
#include <sstream>
#include <iomanip>

// ── UTF-8 변환 헬퍼 ──────────────────────────────────────────

std::string Repository::ToUtf8(const std::wstring& ws)
{
    if (ws.empty()) return {};
    int sz = WideCharToMultiByte(CP_UTF8, 0, ws.c_str(), -1, nullptr, 0, nullptr, nullptr);
    if (sz <= 1) return {};
    std::string s(sz - 1, 0);
    WideCharToMultiByte(CP_UTF8, 0, ws.c_str(), -1, s.data(), sz, nullptr, nullptr);
    return s;
}

std::wstring Repository::FromUtf8(const char* s)
{
    if (!s || !s[0]) return {};
    int sz = MultiByteToWideChar(CP_UTF8, 0, s, -1, nullptr, 0);
    if (sz <= 1) return {};
    std::wstring ws(sz - 1, 0);
    MultiByteToWideChar(CP_UTF8, 0, s, -1, ws.data(), sz);
    return ws;
}

// LIKE 패턴에서 와일드카드로 해석되는 문자 이스케이프 (ESCAPE '\' 절과 함께 사용)
static std::string EscapeLike(const std::string& s)
{
    std::string out;
    out.reserve(s.size());
    for (char c : s) {
        if (c == '%' || c == '_' || c == '\\') out.push_back('\\');
        out.push_back(c);
    }
    return out;
}

// ── 현재 UTC ISO 8601 문자열 ─────────────────────────────────

static std::string NowIso8601()
{
    SYSTEMTIME st;
    GetSystemTime(&st);
    char buf[32];
    snprintf(buf, sizeof(buf), "%04d-%02d-%02dT%02d:%02d:%02d.000Z",
             st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond);
    return buf;
}

// ── 생성자 / 소멸자 ──────────────────────────────────────────

Repository::Repository() = default;

Repository::~Repository()
{
    if (_stmtInsert)   { sqlite3_finalize(_stmtInsert);   _stmtInsert   = nullptr; }
    if (_stmtFindHash) { sqlite3_finalize(_stmtFindHash); _stmtFindHash = nullptr; }
    if (_db) { sqlite3_close(_db); _db = nullptr; }
}

// ── 초기화 ───────────────────────────────────────────────────

bool Repository::Initialize()
{
    const std::string path = ToUtf8(GetClipEverythingDatabasePath());

    if (sqlite3_open_v2(path.c_str(), &_db,
                        SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE, nullptr) != SQLITE_OK)
        return false;

    Execute("PRAGMA journal_mode=WAL;");
    Execute("PRAGMA foreign_keys=ON;");
    Execute(R"(
        CREATE TABLE IF NOT EXISTS ClipboardItems (
            Id              INTEGER PRIMARY KEY AUTOINCREMENT,
            Name            TEXT,
            SourceApp       TEXT NOT NULL DEFAULT '',
            SourceWindow    TEXT NOT NULL DEFAULT '',
            ExecutablePath  TEXT NOT NULL DEFAULT '',
            ContentHash     TEXT NOT NULL DEFAULT '',
            ContentType     INTEGER NOT NULL DEFAULT 0,
            Tags            TEXT NOT NULL DEFAULT '',
            IsFavorite      INTEGER NOT NULL DEFAULT 0,
            CreatedAt       TEXT NOT NULL,
            LastCopiedAt    TEXT NOT NULL,
            Thumbnail       BLOB
        );
        CREATE TABLE IF NOT EXISTS ClipboardFormats (
            Id          INTEGER PRIMARY KEY AUTOINCREMENT,
            ItemId      INTEGER NOT NULL
                            REFERENCES ClipboardItems(Id) ON DELETE CASCADE,
            FormatId    INTEGER NOT NULL,
            FormatName  TEXT NOT NULL,
            Data        BLOB NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_items_hash    ON ClipboardItems(ContentHash);
        CREATE INDEX IF NOT EXISTS idx_items_source  ON ClipboardItems(SourceApp);
        CREATE INDEX IF NOT EXISTS idx_items_created ON ClipboardItems(LastCopiedAt DESC);
        CREATE INDEX IF NOT EXISTS idx_formats_item  ON ClipboardFormats(ItemId);
    )");

    // Prepared statement 사전 컴파일
    sqlite3_prepare_v2(_db,
        "SELECT Id FROM ClipboardItems WHERE ContentHash=? LIMIT 1",
        -1, &_stmtFindHash, nullptr);

    sqlite3_prepare_v2(_db, R"(
        INSERT INTO ClipboardItems
            (Name, SourceApp, SourceWindow, ExecutablePath,
             ContentHash, ContentType, Tags, IsFavorite,
             CreatedAt, LastCopiedAt, Thumbnail)
        VALUES (NULL,?,?,?,?,?,''  ,0,?,?,?)
    )", -1, &_stmtInsert, nullptr);

    return true;
}

void Repository::Execute(const char* sql)
{
    sqlite3_exec(_db, sql, nullptr, nullptr, nullptr);
}

// ── 중복 감지 ────────────────────────────────────────────────

int64_t Repository::FindByHash(const std::string& hash)
{
    sqlite3_reset(_stmtFindHash);
    sqlite3_bind_text(_stmtFindHash, 1, hash.c_str(), -1, SQLITE_STATIC);
    if (sqlite3_step(_stmtFindHash) == SQLITE_ROW)
        return sqlite3_column_int64(_stmtFindHash, 0);
    return 0;
}

// ── InsertItem ───────────────────────────────────────────────

int64_t Repository::InsertItem(const ClipboardSnapshot& snap, const SourceInfo& src, const std::string& now)
{
    sqlite3_reset(_stmtInsert);
    auto app  = ToUtf8(src.processName);
    auto win  = ToUtf8(src.windowTitle);
    auto exe  = ToUtf8(src.exePath);
    sqlite3_bind_text(_stmtInsert, 1, app.c_str(),          -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(_stmtInsert, 2, win.c_str(),          -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(_stmtInsert, 3, exe.c_str(),          -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(_stmtInsert, 4, snap.contentHash.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int (_stmtInsert, 5, (int)snap.contentType);
    sqlite3_bind_text(_stmtInsert, 6, now.c_str(),          -1, SQLITE_TRANSIENT);
    sqlite3_bind_text(_stmtInsert, 7, now.c_str(),          -1, SQLITE_TRANSIENT);
    if (snap.thumbnail.empty())
        sqlite3_bind_null(_stmtInsert, 8);
    else
        sqlite3_bind_blob(_stmtInsert, 8, snap.thumbnail.data(), (int)snap.thumbnail.size(), SQLITE_TRANSIENT);

    sqlite3_step(_stmtInsert);
    return sqlite3_last_insert_rowid(_db);
}

// ── SaveOrUpdate ─────────────────────────────────────────────

int64_t Repository::SaveOrUpdate(const ClipboardSnapshot& snap, const SourceInfo& src)
{
    // 중복 감지: 같은 내용 재복사 시 시각만 갱신
    int64_t existing = FindByHash(snap.contentHash);
    if (existing > 0) {
        std::string now = NowIso8601();
        sqlite3_stmt* stmt = nullptr;
        sqlite3_prepare_v2(_db,
            "UPDATE ClipboardItems SET LastCopiedAt=? WHERE Id=?", -1, &stmt, nullptr);
        sqlite3_bind_text (stmt, 1, now.c_str(), -1, SQLITE_TRANSIENT);
        sqlite3_bind_int64(stmt, 2, existing);
        sqlite3_step(stmt);
        sqlite3_finalize(stmt);
        return existing;
    }

    std::string now = NowIso8601();

    // 아이템 + 포맷 저장을 하나의 트랜잭션으로 (원자성 + fsync 1회)
    Execute("BEGIN IMMEDIATE;");

    int64_t itemId = InsertItem(snap, src, now);

    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(_db,
        "INSERT INTO ClipboardFormats (ItemId,FormatId,FormatName,Data) VALUES(?,?,?,?)",
        -1, &stmt, nullptr);
    for (auto& fmt : snap.formats) {
        sqlite3_reset(stmt);
        auto fname = ToUtf8(fmt.formatName);
        sqlite3_bind_int64(stmt, 1, itemId);
        sqlite3_bind_int64(stmt, 2, fmt.formatId);
        sqlite3_bind_text (stmt, 3, fname.c_str(), -1, SQLITE_TRANSIENT);
        sqlite3_bind_blob (stmt, 4, fmt.data.data(), (int)fmt.data.size(), SQLITE_TRANSIENT);
        sqlite3_step(stmt);
    }
    sqlite3_finalize(stmt);

    Execute("COMMIT;");

    return itemId;
}

// ── GetItems ─────────────────────────────────────────────────

static constexpr const char* kItemColumns =
    "Id, Name, SourceApp, SourceWindow, ExecutablePath,"
    " ContentHash, ContentType, Tags, IsFavorite,"
    " CreatedAt, LastCopiedAt, Thumbnail";

ClipboardItem Repository::ReadItemRow(sqlite3_stmt* stmt)
{
    ClipboardItem item;
    item.id = sqlite3_column_int64(stmt, 0);
    if (sqlite3_column_type(stmt, 1) != SQLITE_NULL)
        item.name = FromUtf8((const char*)sqlite3_column_text(stmt, 1));
    item.sourceApp    = FromUtf8((const char*)sqlite3_column_text(stmt, 2));
    item.sourceWindow = FromUtf8((const char*)sqlite3_column_text(stmt, 3));
    item.exePath      = FromUtf8((const char*)sqlite3_column_text(stmt, 4));
    if (auto* hash = (const char*)sqlite3_column_text(stmt, 5))
        item.contentHash = hash;
    item.contentType  = (ContentType)sqlite3_column_int(stmt, 6);
    item.tags         = FromUtf8((const char*)sqlite3_column_text(stmt, 7));
    item.isFavorite   = sqlite3_column_int(stmt, 8) != 0;
    item.createdAt    = FromUtf8((const char*)sqlite3_column_text(stmt, 9));
    item.lastCopiedAt = FromUtf8((const char*)sqlite3_column_text(stmt, 10));
    if (sqlite3_column_type(stmt, 11) != SQLITE_NULL) {
        int sz = sqlite3_column_bytes(stmt, 11);
        auto* blob = (const uint8_t*)sqlite3_column_blob(stmt, 11);
        item.thumbnail.assign(blob, blob + sz);
    }
    return item;
}

std::vector<ClipboardItem> Repository::GetItems(const std::wstring& search, const std::wstring& app)
{
    std::string sql = std::string("SELECT ") + kItemColumns + " FROM ClipboardItems";

    std::vector<std::string> wheres;
    std::vector<std::string> params;

    if (!search.empty()) {
        auto s = ToUtf8(search);
        if (!s.empty() && s[0] == '#') {
            wheres.push_back("(',' || Tags || ',') LIKE ? ESCAPE '\\'");
            params.push_back("%" + EscapeLike(s) + ",%");
        } else if (!s.empty()) {
            wheres.push_back("(Name LIKE ? ESCAPE '\\' OR SourceWindow LIKE ? ESCAPE '\\' OR SourceApp LIKE ? ESCAPE '\\')");
            std::string q = "%" + EscapeLike(s) + "%";
            params.push_back(q); params.push_back(q); params.push_back(q);
        }
    }
    if (!app.empty()) {
        wheres.push_back("SourceApp = ?");
        params.push_back(ToUtf8(app));
    }

    if (!wheres.empty()) {
        sql += " WHERE ";
        for (size_t i = 0; i < wheres.size(); ++i) {
            if (i > 0) sql += " AND ";
            sql += wheres[i];
        }
    }
    sql += " ORDER BY IsFavorite DESC, LastCopiedAt DESC LIMIT 500";

    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(_db, sql.c_str(), -1, &stmt, nullptr);

    int col = 1;
    for (auto& p : params)
        sqlite3_bind_text(stmt, col++, p.c_str(), -1, SQLITE_TRANSIENT);

    std::vector<ClipboardItem> items;
    while (sqlite3_step(stmt) == SQLITE_ROW)
        items.push_back(ReadItemRow(stmt));
    sqlite3_finalize(stmt);
    return items;
}

// ── GetItemById ──────────────────────────────────────────────

std::optional<ClipboardItem> Repository::GetItemById(int64_t id)
{
    const std::string sql = std::string("SELECT ") + kItemColumns +
                            " FROM ClipboardItems WHERE Id=? LIMIT 1";
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(_db, sql.c_str(), -1, &stmt, nullptr);
    sqlite3_bind_int64(stmt, 1, id);

    std::optional<ClipboardItem> item;
    if (sqlite3_step(stmt) == SQLITE_ROW)
        item = ReadItemRow(stmt);
    sqlite3_finalize(stmt);
    return item;
}

// ── GetFormats ───────────────────────────────────────────────

std::vector<ClipboardFormat> Repository::GetFormats(int64_t itemId)
{
    sqlite3_stmt* stmt = nullptr;
    sqlite3_prepare_v2(_db,
        "SELECT Id,ItemId,FormatId,FormatName,Data FROM ClipboardFormats WHERE ItemId=? ORDER BY Id ASC",
        -1, &stmt, nullptr);
    sqlite3_bind_int64(stmt, 1, itemId);

    std::vector<ClipboardFormat> fmts;
    while (sqlite3_step(stmt) == SQLITE_ROW) {
        ClipboardFormat f;
        f.id         = sqlite3_column_int64(stmt, 0);
        f.itemId     = sqlite3_column_int64(stmt, 1);
        f.formatId   = (uint32_t)sqlite3_column_int64(stmt, 2);
        f.formatName = FromUtf8((const char*)sqlite3_column_text(stmt, 3));
        int sz = sqlite3_column_bytes(stmt, 4);
        auto* blob = (const uint8_t*)sqlite3_column_blob(stmt, 4);
        f.data.assign(blob, blob + sz);
        fmts.push_back(std::move(f));
    }
    sqlite3_finalize(stmt);
    return fmts;
}

// ── UPDATE / DELETE ──────────────────────────────────────────

void Repository::Rename(int64_t id, const std::wstring& name)
{
    sqlite3_stmt* s = nullptr;
    sqlite3_prepare_v2(_db, "UPDATE ClipboardItems SET Name=? WHERE Id=?", -1, &s, nullptr);
    auto n = ToUtf8(name);
    sqlite3_bind_text(s, 1, n.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int64(s, 2, id);
    sqlite3_step(s); sqlite3_finalize(s);
}

void Repository::SetFavorite(int64_t id, bool fav)
{
    sqlite3_stmt* s = nullptr;
    sqlite3_prepare_v2(_db, "UPDATE ClipboardItems SET IsFavorite=? WHERE Id=?", -1, &s, nullptr);
    sqlite3_bind_int(s, 1, fav ? 1 : 0);
    sqlite3_bind_int64(s, 2, id);
    sqlite3_step(s); sqlite3_finalize(s);
}

void Repository::SetTags(int64_t id, const std::wstring& tags)
{
    sqlite3_stmt* s = nullptr;
    sqlite3_prepare_v2(_db, "UPDATE ClipboardItems SET Tags=? WHERE Id=?", -1, &s, nullptr);
    auto t = ToUtf8(tags);
    sqlite3_bind_text(s, 1, t.c_str(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int64(s, 2, id);
    sqlite3_step(s); sqlite3_finalize(s);
}

void Repository::Delete(int64_t id)
{
    sqlite3_stmt* s = nullptr;
    sqlite3_prepare_v2(_db, "DELETE FROM ClipboardItems WHERE Id=?", -1, &s, nullptr);
    sqlite3_bind_int64(s, 1, id);
    sqlite3_step(s); sqlite3_finalize(s);
}

void Repository::DeleteAll()
{
    Execute("DELETE FROM ClipboardItems");
}
