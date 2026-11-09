import { useEffect, useState } from "react";
import type { DownloadClient, JobDto } from "../ipc/types";

/** Jobs from `list_jobs`, refreshed on backend job events (at most every 400 ms). */
export function useJobs(client: DownloadClient): JobDto[] {
  const [jobs, setJobs] = useState<JobDto[]>([]);
  useEffect(() => {
    let live = true;
    let unlisten: (() => void) | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let last = 0;
    const load = () => {
      timer = null;
      last = Date.now();
      client.listJobs().then(
        (j) => live && setJobs(j),
        () => {},
      );
    };
    const schedule = () => {
      if (timer) return;
      timer = setTimeout(load, Math.max(0, 400 - (Date.now() - last)));
    };
    load();
    client.onJobChanged(() => live && schedule()).then(
      (u) => (live ? (unlisten = u) : u()),
      () => {},
    );
    return () => {
      live = false;
      if (timer) clearTimeout(timer);
      unlisten?.();
    };
  }, [client]);
  return jobs;
}

const OPEN = new Set(["queued", "blocked", "running", "retry_wait", "pausing", "paused", "canceling"]);
export const isOpen = (j: JobDto) => OPEN.has(j.status);
