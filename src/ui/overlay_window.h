#pragma once
#define WIN32_LEAN_AND_MEAN

#include <windows.h>
#include <commctrl.h>
#include <dwmapi.h>

#include <cstdint>
#include <string>
#include <vector>

#include "../data/models.h"
#include "../data/repository.h"
#include "../services/clipboard_service.h"
#include "render/image_cache.h"

struct AppSettings;

class OverlayWindow {
public:
    static bool RegisterClass(HINSTANCE hInst);

    OverlayWindow(HINSTANCE hInst, Repository& repo, ClipboardService& svc, AppSettings& settings);
    ~OverlayWindow();

    void ShowAndRefresh(const std::wstring& contextApp);
    void ShowAndEditItem(int64_t itemId);
    void Hide();

    HWND GetHwnd() const { return _hwnd; }

private:
    enum class InlineEditorMode {
        None,
        Rename,
        TagEdit,
    };

    enum class ActionTarget {
        None,
        Body,
        ToggleAll,
        Close,
        Rename,
        Favorite,
        Delete,
        ConfirmDelete,   // 인라인 삭제 확인: 삭제 확정
        CancelDelete,    // 인라인 삭제 확인: 취소
        TagAdd,
        TagChip,
        TagRemove,
    };

    enum class WindowDragMode {
        None,
        Move,
        ResizeTop,
        ResizeBottom,
    };

    struct ActionHit {
        ActionTarget type = ActionTarget::None;
        int cardIdx = -1;
        int tagIdx = -1;
        D2D1_RECT_F rect{};
    };

    static LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp);
    static LRESULT CALLBACK EditPanelWndProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp);
    static LRESULT CALLBACK EditSubclassProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp,
                                             UINT_PTR subclassId, DWORD_PTR refData);

    LRESULT HandleMessage(UINT msg, WPARAM wp, LPARAM lp);

    void OnCreate();
    void OnPaint();
    void OnSize(int w, int h);
    void OnMouseMove(int x, int y);
    void OnMouseLeave();
    void OnLButtonDown(int x, int y);
    void OnLButtonUp(int x, int y);
    void OnMouseWheel(int delta);
    void OnVScroll(WPARAM code, int pos);
    void OnKeyDown(WPARAM vk);
    void OnSearchChanged();
    void OnActivate(bool active);
    void OnDpiChanged(int dpi, const RECT* suggested);
    void OnTimer(UINT_PTR timerId);
    LRESULT OnSetCursor();

    void LoadItems();
    void ShowOverlay(const std::wstring& contextApp, int64_t editItemId);
    void DrawCard(ID2D1HwndRenderTarget* rt, const ClipboardItem& item,
                  int itemIdx, float x, float y, float w, float h,
                  bool hover, bool selected);
    void ApplyWindowEffect();
    void PositionWindow(bool activate);
    void SaveWindowBounds();
    void ClampScrollOffset();
    void SyncScrollBar();
    float GetListTopPx() const;
    float GetListHeightPx() const;
    float GetMaxScrollPx() const;
    RECT GetDefaultBounds() const;
    RECT GetPreferredBounds() const;
    RECT ClampBoundsToWorkArea(const RECT& bounds) const;
    bool IsPointInResizeTopZone(int y) const;
    bool IsPointInResizeBottomZone(int y, int clientHeight) const;
    void SetSelectedIndex(int idx);
    int FindItemIndexById(int64_t itemId) const;
    int HitTestCard(int mouseY) const;
    ActionHit HitTestAction(int x, int y) const;
    void ExecutePaste(int64_t itemId);
    std::wstring GetTooltipText(const ActionHit& hit) const;
    std::wstring GetHoverStatusText() const;
    void ResetTooltip(bool invalidate);
    void UpdateHoverState(int x, int y);
    bool IsSameActionHit(const ActionHit& a, const ActionHit& b) const;

    void BeginInlineRename(int itemIdx);
    void CommitInlineRename(bool closeOverlay);
    void CancelInlineRename(bool closeOverlay);
    void BeginInlineTagEdit(int itemIdx, int tagIdx);
    void CommitInlineTagEdit();
    void CancelInlineTagEdit();
    void CommitActiveInlineEditor(bool closeOverlay);
    void EndInlineEditorSession();
    void UpdateInlineEditorLayout();
    void SyncInlineEditIme(HWND edit);
    void RefreshInlineEditActivation();
    bool IsInlineRenameActive() const;
    bool IsInlineTagActive() const;
    bool IsInlineEditorActive() const;
    bool IsEditingItem(int itemIdx) const;

    HWND _hwnd = nullptr;
    HWND _hSearch = nullptr;
    HWND _hNamePanel = nullptr;
    HWND _hNameEdit = nullptr;
    HWND _hTagPanel = nullptr;
    HWND _hTagEdit = nullptr;

    HFONT _hSearchFont = nullptr;
    HFONT _hNameEditFont = nullptr;
    HFONT _hTagEditFont = nullptr;
    HBRUSH _hNameEditBrush = nullptr;
    HBRUSH _hTagEditBrush = nullptr;

    HINSTANCE _hInst = nullptr;
    Repository& _repo;
    ClipboardService& _svc;
    AppSettings& _settings;
    ImageCache _imgCache;

    std::vector<ClipboardItem> _items;
    std::wstring _contextApp;
    bool _showAll = false;

    int _hoverIdx = -1;
    int _selectedIdx = -1;
    float _scrollOffset = 0.0f;
    float _contentHeight = 0.0f;
    int _dpi = 96;

    bool _isClosing = false;
    bool _mouseTracked = false;
    InlineEditorMode _inlineEditorMode = InlineEditorMode::None;
    int64_t _editingItemId = 0;
    int _editingTagIdx = -1;
    int64_t _confirmDeleteItemId = 0; // 0이 아니면 해당 항목에 삭제 확인 UI 표시
    ActionHit _hoverAction;
    ActionHit _pressedAction;
    ActionHit _tooltipAction;
    bool _tooltipVisible = false;
    POINT _lastMousePos{ -1, -1 };
    WindowDragMode _dragMode = WindowDragMode::None;
    POINT _dragStartScreen{};
    RECT _dragStartRect{};

    static constexpr wchar_t kClassName[] = L"ClipEverythingOverlay";
};
