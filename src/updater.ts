import { check, type Update } from "@tauri-apps/plugin-updater";

export type { Update };

export async function checkForUpdates(): Promise<Update | null> {
  try {
    const update = await check();
    return update ?? null;
  } catch {
    // Silently fail — updater may not be configured yet (no pubkey, no endpoint, dev mode)
    return null;
  }
}
