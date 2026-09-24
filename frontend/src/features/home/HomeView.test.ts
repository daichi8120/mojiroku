import { describe, expect, it } from "vitest";
import { upcomingEvents } from "./HomeView";

describe("upcomingEvents (#115)", () => {
  const now = new Date("2026-09-24T10:30:00").getTime();
  const ev = (id: string, start: string, end: string | null) => ({ id, title: id, start, end, location: null });
  it("keeps future and ongoing events and drops finished ones", () => {
    const got = upcomingEvents(
      [
        ev("done", "2026-09-24T09:00:00", "2026-09-24T10:00:00"),
        ev("ongoing", "2026-09-24T10:00:00", "2026-09-24T11:00:00"),
        ev("later", "2026-09-24T15:00:00", "2026-09-24T16:00:00"),
      ],
      now,
    ).map((e) => e.id);
    expect(got).toEqual(["ongoing", "later"]);
  });
  it("treats an event without an end as ongoing for an hour", () => {
    expect(upcomingEvents([ev("open", "2026-09-24T10:00:00", null)], now)).toHaveLength(1);
    expect(upcomingEvents([ev("old", "2026-09-24T09:00:00", null)], now)).toHaveLength(0);
  });
});
