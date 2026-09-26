// この Mac だけの小さな表示設定（#115）。localStorage が使えないときは既定値で動く。
const DIARIZE_KEY = "mojiroku.diarizeByDefault";
const SIDEBAR_KEY = "mojiroku.sidebarCollapsed";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* 保存できなくても今回の操作はそのまま効く */
  }
}

/** ファイル取り込み・マイク録音・後から文字起こしで話者分離をするか。既定 ON、最後の選択を覚える。 */
export const getDiarizePref = () => read(DIARIZE_KEY) !== "0";
export const setDiarizePref = (on: boolean) => write(DIARIZE_KEY, on ? "1" : "0");

/** サイドバーを細く畳んでいるか。 */
export const getSidebarCollapsed = () => read(SIDEBAR_KEY) === "1";
export const setSidebarCollapsed = (on: boolean) => write(SIDEBAR_KEY, on ? "1" : "0");
