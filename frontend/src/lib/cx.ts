/** クラス名を結合する小さなヘルパー（falsy は無視）。 */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
