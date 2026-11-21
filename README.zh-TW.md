# ArcDockWin（繁體中文）

**為 Windows 11 打造的 macOS 風格 Dock 與選單列，使用 Rust 撰寫。**

ArcDockWin 以原生 Win32、Direct2D 與 Windows Composition 實作，無須額外執行階段。它可取代日常使用的 Windows 工作列，提供 Dock、頂端選單列與控制中心。

## 繁體中文與語言切換

ArcDockWin 會讀取 Windows 使用者的 UI 顯示語言。Windows 使用中文介面時，程式自動套用內建的繁體中文（台灣）語言包；其他語言則使用專案原有的葡萄牙文介面。程式每秒重新檢查一次 Windows 語言，偵測到變更後會更新選單列、已開啟的面板、日曆和選單，不必重新啟動。翻譯表位於 [`locales/zh-TW.tsv`](locales/zh-TW.tsv)。

## 功能

### Dock

- 半透明模糊玻璃底板、圓角與柔和陰影。
- 指標移過圖示時會放大；執行中的 App 會顯示 macOS 風格的小型狀態點。
- 放大效果會平滑跟隨指標，減少微小抖動；在有多個視窗的 App 圖示上滾動滑鼠滾輪可切換視窗。
- App 要求注意時圖示會跳動；開啟右鍵選單可固定／取消固定 App 或關閉視窗。
- 可拖曳圖示排序；Launcher 永遠位於 Dock 項目末端。
- Dock 尾端附有「下載項目」堆疊與垃圾桶，支援自動隱藏，並跟隨 Windows 淺色／深色模式。
- 可透過 `icons.txt` 將 Dock 圖示換成 MacTahoe 圖示主題中的 `.ico`。

### 頂端選單列與控制中心

- 每個螢幕都有選單列，包含日期／時間、CPU／記憶體、系統匣圖示及控制中心。
- 控制中心提供 Wi‑Fi、藍牙、淺色／深色模式、電源、亮度、音量與媒體控制。
- 系統匣圖示可選擇固定在選單列或收納到溢位面板。

### 檔案總管的 Tahoe 圖示

圖示套用程式會將 MacTahoe 主題延伸到 Windows 檔案總管：一般與開啟中的資料夾、桌面、文件、下載、圖片、音樂、影片、磁碟、此電腦和垃圾桶都可使用相應的主題圖示。它透過 Windows Shell 圖示登錄設定套用，並保留備份供還原；不會注入或修補 Explorer 程序。檔案總管仍保留 Windows 原生的視窗與操作方式。

## 安裝與編譯

### 使用已編譯版本

從 [GitHub Releases](https://github.com/claudsondouglas/ArcDockWin/releases/latest) 下載 `arcdockwin.exe`。建議放在固定資料夾，例如 `C:\Programas\ArcDockWin`，再啟動程式。需求：Windows 11。

### 從原始碼編譯

需求：Windows 11、Rust MSVC 工具鏈與 Visual Studio C++ Build Tools。

```powershell
git clone https://github.com/claudsondouglas/ArcDockWin.git
cd ArcDockWin
cargo build --release
.\target\release\arcdockwin.exe
```

程式首次執行時會登錄為使用者登入後啟動。退出方式：在 Dock 空白處按右鍵，選擇「結束 ArcDockWin」；Windows 工作列會同步還原。

## 安裝 MacTahoe 圖示

1. 安裝 ImageMagick：

   ```powershell
   winget install ImageMagick.ImageMagick
   ```

2. 在新開啟的 PowerShell 執行轉換腳本：

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\icons\Converter.ps1
   ```

   預設輸出在 `C:\Icones\MacTahoe`，包含 `apps\`、`sistema\`、`aliases.txt` 和套用腳本。

3. 以系統管理員 PowerShell 套用檔案總管及桌面圖示：

   ```powershell
   cd C:\Icones\MacTahoe
   powershell -ExecutionPolicy Bypass -File .\IconesTahoe.ps1 -Testar
   powershell -ExecutionPolicy Bypass -File .\IconesTahoe.ps1 -Instalar
   ```

   `-Testar` 只預覽捷徑的圖示配對；`-Instalar` 會要求 UAC 權限，並建立登入後執行的排程工作。右下角系統匣選單可立即重新套用、設定自動套用間隔或還原 Windows 圖示。

個別 Dock 圖示也可在 `%APPDATA%\ArcDockWin\icons.txt` 指定，例如：

```text
spotify.exe=C:\Icones\MacTahoe\apps\spotify.ico
```

圖示主題不會隨此專案一併散布；轉換腳本會從 [MacTahoe icon theme](https://github.com/vinceliuice/MacTahoe-icon-theme) 下載，並依原專案 GPL-3.0 授權。

## 設定與還原

ArcDockWin 的使用者設定位於 `%APPDATA%\ArcDockWin\`：`pinned.txt` 管理 Dock 固定項目，`icons.txt` 管理自訂圖示，`colors.db` 儲存選單列 App 色彩與系統匣顯示選項。

要還原 Explorer、捷徑及磁碟的原始圖示：

```powershell
powershell -ExecutionPolicy Bypass -File C:\Icones\MacTahoe\IconesTahoe.ps1 -Desinstalar
```

圖示腳本會從備份復原原始登錄值與捷徑圖示，並移除排程工作。

## 授權

ArcDockWin 使用 MIT License。MacTahoe 圖示主題由其作者另行提供，依 GPL-3.0 授權。ArcDockWin 受 macOS 設計啟發，與 Apple 無隸屬關係。
