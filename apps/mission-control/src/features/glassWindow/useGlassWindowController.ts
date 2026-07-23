import { useCallback, useEffect, useRef, useState } from "react";

import {
  getFloorPresence,
  getOfficeChatter,
  postOfficeChatterMessage,
} from "../../glass/window/api";
import type {
  FloorPresenceResponse,
  OfficeChatterResponse,
} from "../../glass/window/types";
import type { RuntimeConnectionSettings } from "../../types";

const DEFAULT_REFRESH_MS = 5_000;

export function useGlassWindowController(props: {
  active: boolean;
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
}) {
  const { active, settings, tokenConfigured } = props;
  const [presence, setPresence] = useState<FloorPresenceResponse | null>(null);
  const [chatter, setChatter] = useState<OfficeChatterResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const refreshPromise = useRef<Promise<boolean> | null>(null);
  const requestGeneration = useRef(0);

  const refresh = useCallback((): Promise<boolean> => {
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      requestGeneration.current += 1;
      refreshPromise.current = null;
      setPresence(null);
      setChatter(null);
      setError("Connect CarsinOS to observe the Window.");
      setLoading(false);
      return Promise.resolve(false);
    }
    if (refreshPromise.current) {
      return refreshPromise.current;
    }
    setLoading(true);
    const generation = requestGeneration.current;
    const pending = Promise.all([
      getFloorPresence(settings),
      getOfficeChatter(settings),
    ])
      .then(([nextPresence, nextChatter]) => {
        if (generation !== requestGeneration.current) return false;
        setPresence(nextPresence);
        setChatter(nextChatter);
        setError(null);
        return true;
      })
      .catch((reason: unknown) => {
        if (generation !== requestGeneration.current) return false;
        setError(
          reason instanceof Error
            ? reason.message
            : "The Window is not wired yet.",
        );
        return false;
      })
      .finally(() => {
        if (generation === requestGeneration.current) {
          setLoading(false);
        }
        if (refreshPromise.current === pending) {
          refreshPromise.current = null;
        }
      });
    refreshPromise.current = pending;
    return pending;
  }, [settings, tokenConfigured]);

  useEffect(() => {
    if (!active) return;
    const initial = window.setTimeout(() => void refresh(), 0);
    const refreshMs = Math.max(
      2_000,
      Math.min(presence?.refresh_after_ms ?? DEFAULT_REFRESH_MS, 30_000),
    );
    const timer = window.setInterval(() => void refresh(), refreshMs);
    return () => {
      window.clearTimeout(initial);
      window.clearInterval(timer);
    };
  }, [active, presence?.refresh_after_ms, refresh]);

  const sendMessage = useCallback(
    async (threadId: string, bodyText: string): Promise<boolean> => {
      const trimmed = bodyText.trim();
      if (!trimmed) return false;
      try {
        await postOfficeChatterMessage(
          settings,
          threadId,
          trimmed,
        );
      } catch (reason) {
        setError(
          reason instanceof Error
            ? reason.message
            : "That note could not be sent.",
        );
        return false;
      }
      try {
        setChatter(await getOfficeChatter(settings));
        setError(null);
      } catch {
        setError("Note sent, but the chatter feed could not refresh.");
      }
      return true;
    },
    [settings],
  );

  return {
    presence,
    chatter,
    error,
    loading,
    refresh,
    sendMessage,
  };
}

export type GlassWindowController = ReturnType<
  typeof useGlassWindowController
>;
