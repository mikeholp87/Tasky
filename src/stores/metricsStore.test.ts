import { beforeEach, describe, expect, it } from "vitest";
import { makeTick } from "@/test/fixtures";
import { useMetricsStore } from "./metricsStore";

describe("metrics ordering", () => {
  beforeEach(() => useMetricsStore.setState({ tick: null }));

  it("does not replace a channel tick with a stale subscription response", () => {
    const store = useMetricsStore.getState();
    const newest = makeTick([], { seq: 5 });
    store.ingestTick(newest);
    const version = useMetricsStore.getState().historyVersion;
    store.ingestTick(makeTick([], { seq: 4 }));
    store.ingestTick(makeTick([], { seq: 5 }));
    expect(useMetricsStore.getState().tick).toBe(newest);
    expect(useMetricsStore.getState().historyVersion).toBe(version);
  });
});
