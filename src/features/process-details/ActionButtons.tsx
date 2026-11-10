import { FolderOpen, OctagonX, Pause, Play, Square } from "lucide-react";
import { outlineButton } from "@/components/ui/button";
import { requestAction, revealExecutable } from "@/features/processes/actions";
import type { ProcessRow } from "@/types";

export function ActionButtons({ row }: { row: ProcessRow }) {
  const suspended = row.status === "stopped";
  return (
    <>
      <button
        type="button"
        className={outlineButton}
        disabled={!row.exe}
        onClick={() => void revealExecutable(row)}
      >
        <FolderOpen size="0.875rem" aria-hidden /> Open location
      </button>
      <button
        type="button"
        className={outlineButton}
        onClick={() => void requestAction(row, { type: suspended ? "resume" : "suspend" })}
      >
        {suspended ? <Play size="0.875rem" aria-hidden /> : <Pause size="0.875rem" aria-hidden />}
        {suspended ? "Resume" : "Suspend"}
      </button>
      <button
        type="button"
        className={outlineButton}
        onClick={() => void requestAction(row, { type: "terminate" })}
      >
        <Square size="0.8125rem" aria-hidden /> End process
      </button>
      <button
        type="button"
        className={`${outlineButton} text-crit hover:text-crit`}
        onClick={() => void requestAction(row, { type: "forceKill" })}
      >
        <OctagonX size="0.875rem" aria-hidden /> Force end
      </button>
    </>
  );
}
