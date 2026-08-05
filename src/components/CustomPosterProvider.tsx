import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  assignGeneratedPoster,
  listCustomPosters,
  type CustomPoster,
} from '../lib/imageGeneration';

type CustomPosterContextValue = {
  posters: Record<string, CustomPoster>;
  assignPoster: (itemId: string, title: string, itemType: string, localPath: string) => Promise<CustomPoster>;
  refreshPosters: () => Promise<void>;
};

const CustomPosterContext = createContext<CustomPosterContextValue | null>(null);

export function CustomPosterProvider({ children }: { children: ReactNode }) {
  const [posters, setPosters] = useState<Record<string, CustomPoster>>({});

  const refreshPosters = useCallback(async () => {
    const items = await listCustomPosters();
    setPosters(Object.fromEntries(items.map((item) => [item.itemId, item])));
  }, []);

  useEffect(() => {
    let cancelled = false;
    listCustomPosters()
      .then((items) => {
        if (!cancelled) setPosters(Object.fromEntries(items.map((item) => [item.itemId, item])));
      })
      .catch(() => {
        // Browser-only development does not expose native poster storage.
      });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    function storageChanged() {
      void refreshPosters().catch(() => setPosters({}));
    }
    window.addEventListener('matinee:artwork-storage-change', storageChanged);
    return () => window.removeEventListener('matinee:artwork-storage-change', storageChanged);
  }, [refreshPosters]);

  const assignPoster = useCallback(async (itemId: string, title: string, itemType: string, localPath: string) => {
    const poster = await assignGeneratedPoster(itemId, title, itemType, localPath);
    setPosters((current) => ({ ...current, [itemId]: poster }));
    return poster;
  }, []);
  const value = useMemo<CustomPosterContextValue>(() => ({ posters, assignPoster, refreshPosters }), [assignPoster, posters, refreshPosters]);

  return <CustomPosterContext.Provider value={value}>{children}</CustomPosterContext.Provider>;
}

export function useCustomPosters() {
  const context = useContext(CustomPosterContext);
  if (!context) throw new Error('useCustomPosters must be used within CustomPosterProvider.');
  return context;
}
