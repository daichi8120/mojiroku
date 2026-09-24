// 詳細画面が今表示しているジョブ（#107 レビュー）。失敗はそこに赤枠で出るので、App は
// そのジョブの失敗トーストを出さない。表示前（詳細がまだジョブを受け取っていない間）に
// 失敗したときは、ここに無いのでトーストが出る。
let shown: string | null = null;

export const setShownJob = (jobId: string | null) => {
  shown = jobId;
};
export const isJobShown = (jobId: string) => shown === jobId;
