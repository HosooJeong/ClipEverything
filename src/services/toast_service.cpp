#include "toast_service.h"
#include "../ui/toast_popup.h"

void ShowToast(HINSTANCE hInst, const ClipboardItem& item)
{
    ShowToastPopup(hInst, item);
}
