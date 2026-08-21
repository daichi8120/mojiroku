// ジョブの経過時間の起点を job_id ごとにアプリ内（モジュール singleton）で保持する。
// core は多くの段で incremental % を出さないため（(stage,0,None)）、経過時間が「処理が
// 止まっていない」唯一の可視シグナル。起点を DetailView の state に持つと画面遷移で
// remount → リセットするので、ビューから独立したここへ逃がす（App が running を最初に
// 観測した時刻を種にし、どの画面へ移っても同じ起点を返す）。
//
// 注意: 真のジョブ開始時刻（サーバ）ではなく「アプリが running を初観測した時刻」。
// 起動直後にすでに走っていたジョブは種を持たないので初観測時に採る（アプリ再起動を
// またいだ継続表示はしない＝そこまでの厳密さは不要）。
const startedAt = new Map<string, number>();
// ステージ内の実進捗アンカー（ETA 用）。キーは `${jobId}:${stage}`。
// whisper の 0-100% は transcribe ステージ内で単調に増えるので、その**最初の実 tick**
// （done>0）の時刻を起点に「残り約N分」を線形外挿する。ステージ入場ではなく最初の実 tick を
// 起点にするのは、モデル読込(547MB)/state 生成/VAD の固定オーバーヘッドを ETA に含めない
// ため（序盤で「残り」が過大に膨らむのを防ぐ・advisor）。
const stageStartedAt = new Map<string, number>();

const stageKey = (jobId: string, stage: string) => `${jobId}:${stage}`;

/** job_id の起点を（無ければ現在時刻で）確定し、その epoch(ms) を返す。冪等。 */
export function markJobStart(jobId: string): number {
  let t = startedAt.get(jobId);
  if (t === undefined) {
    t = Date.now();
    startedAt.set(jobId, t);
  }
  return t;
}

/** job_id の起点を返す（未記録なら undefined）。 */
export function getJobStart(jobId: string): number | undefined {
  return startedAt.get(jobId);
}

/** (job_id, stage) の実進捗アンカーを（無ければ現在時刻で）確定して返す。冪等。 */
export function markStageStart(jobId: string, stage: string): number {
  const key = stageKey(jobId, stage);
  let t = stageStartedAt.get(key);
  if (t === undefined) {
    t = Date.now();
    stageStartedAt.set(key, t);
  }
  return t;
}

/** (job_id, stage) の実進捗アンカーを返す（未記録なら undefined）。 */
export function getStageStart(jobId: string, stage: string): number | undefined {
  return stageStartedAt.get(stageKey(jobId, stage));
}

/** 終了（done/failed/canceled）したジョブの起点とステージアンカーを捨てる（Map の肥大防止）。 */
export function clearJobStart(jobId: string): void {
  startedAt.delete(jobId);
  const prefix = `${jobId}:`;
  for (const key of stageStartedAt.keys()) {
    if (key.startsWith(prefix)) stageStartedAt.delete(key);
  }
}
