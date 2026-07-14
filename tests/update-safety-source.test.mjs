import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const readSource = (relativePath) =>
  readFileSync(new URL(`../${relativePath}`, import.meta.url), 'utf8');

test('update dialog only hands the official release URL to open_url', () => {
  const source = readSource('src/components/UpdateDialog.vue');

  assert.doesNotMatch(source, /downloadUpdate|installUpdate|resolveMirrorUrl/);
  assert.doesNotMatch(source, /@tauri-apps\/api\/event|download-progress/);
  assert.doesNotMatch(source, /window\.open/);
  assert.match(
    source,
    /invoke\('open_url',\s*\{\s*url:\s*props\.updateInfo\.download_url\s*\}\)/,
  );
});

test('renderer updater utility exposes metadata checks only', () => {
  const source = readSource('src/utils/updater.ts');

  assert.match(source, /export async function checkForUpdates\(\): Promise<UpdateInfo>/);
  assert.doesNotMatch(source, /resolveMirrorUrl|downloadUpdate|installUpdate/);
  assert.doesNotMatch(source, /download_update|install_update|getSettings|MIRROR_PRESETS/);
});

test('settings no longer exposes update mirror controls', () => {
  const source = readSource('src/components/Settings.vue');

  assert.doesNotMatch(source, /下载加速镜像|mirror-controls|downloadMirror|downloadMirrorUrl/);
});

test('Tauri command registry does not expose downloader or installer commands', () => {
  const source = readSource('src-tauri/src/lib.rs');

  assert.doesNotMatch(source, /commands::download_update/);
  assert.doesNotMatch(source, /commands::install_update/);
  assert.match(source, /commands::check_for_updates/);
  assert.match(source, /commands::open_url/);
});

test('open_url validates the official release URL and avoids cmd shell execution', () => {
  const source = readSource('src-tauri/src/commands.rs');

  assert.match(source, /fn is_official_release_url\(url: &str\) -> bool/);
  assert.match(source, /is_official_release_url\(&url\)/);
  assert.doesNotMatch(source, /Command::new\("cmd"\)|\.args\(\["\/C",\s*"start"/);
  assert.match(source, /\.opener\(\)\s*\.open_url\(/);
});
