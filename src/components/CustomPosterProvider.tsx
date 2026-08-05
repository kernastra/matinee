import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  assignGeneratedPoster,
  listCustomPosters,
  type CustomPoster,
} from '../lib/imageGeneration';

type CustomPosterContextValue = {
  posters: Record<string, CustomPoster>;
  assignPoster: (itemId: string, localPath: string) => Promise<CustomPoster>;
};

const CustomPosterContext = createContext<CustomPosterContextValue | null>(null);

export function CustomPosterProvider({ children }: { children: ReactNode }) {
  const [posters, setPosters] = useState<Record<string, CustomPoster>>({});

  useEffect(() => {
    let cancelled = false;
    listCustomPosters()
      .then((items) => {
        if (cancelled) return;
        setPosters(Object.fromEntries(items.map((item) => [item.itemId, item])));
      })
      .catch(() => {
        // Browser-only development does not expose native poster storage.
      });
    return () => { cancelled = true; };
  }, []);

  const assignPoster = useCallback(async (itemId: string, localPath: string) => {
    const poster = await assignGeneratedPoster(itemId, localPath);
    setPosters((current) => ({ ...current, [itemId]: poster }));
    return poster;
  }, []);
  const value = useMemo<CustomPosterContextValue>(() => ({ posters, assignPoster }), [assignPoster, posters]);

  return <CustomPosterContext.Provider value={value}>{children}</CustomPosterContext.Provider>;
}

export function useCustomPosters() {
  const context = useContext(CustomPosterContext);
  if (!context) throw new Error('useCustomPosters must be used within CustomPosterProvider.');
  return context;
}
