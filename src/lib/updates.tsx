import { Button } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { notifyError } from "./api";

async function install(update: Update) {
  const id = notifications.show({ loading: true, title: "Updating Grandie Chess", message: "Downloading…", autoClose: false });
  try {
    let received = 0;
    let total = 0;
    await update.downloadAndInstall((e) => {
      if (e.event === "Started") total = e.data.contentLength ?? 0;
      if (e.event === "Progress") {
        received += e.data.chunkLength;
        if (total) notifications.update({ id, message: `Downloading… ${Math.round((received / total) * 100)}%` });
      }
    });
    notifications.update({ id, loading: false, message: "Restarting…" });
    await relaunch();
  } catch (e) {
    notifications.hide(id);
    notifyError("Update failed", e);
  }
}

/** Look for a newer release on GitHub; offers to install it. */
export async function checkForUpdates(manual = false) {
  try {
    const update = await check();
    if (!update) {
      if (manual) notifications.show({ message: "Grandie Chess is up to date.", color: "green" });
      return;
    }
    notifications.show({
      title: `Grandie Chess ${update.version} is available`,
      message: (
        <Button size="xs" mt={6} onClick={() => void install(update)}>
          Install and restart
        </Button>
      ),
      autoClose: false,
    });
  } catch (e) {
    // Offline, or no release published yet: only report when asked.
    if (manual) notifyError("Could not check for updates", e);
  }
}
