import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const readSource = (relativePath) =>
  readFileSync(new URL(`../${relativePath}`, import.meta.url), 'utf8');

test('AI analyzing artwork is only rendered while analysis is active', () => {
  const source = readSource('src/components/AiSuggestions.vue');

  assert.match(
    source,
    /v-else-if="state\.analyzing\s*&&\s*suggestions\.length\s*===\s*0"/,
  );
});

test('only migrate AI suggestions expose the accept action', () => {
  const source = readSource('src/components/AiSuggestionCard.vue');

  assert.match(
    source,
    /const canAccept = computed\(\(\) =>\s*\(props\.suggestion\.action[^)]*\)\.toLowerCase\(\) === 'migrate'\s*\);/,
  );
  assert.match(
    source,
    /<button\s+v-if="canAccept"[\s\S]*?@click="emit\('accept', suggestion\)"/,
  );
});

test('system reclaim blocks operations that require missing elevation', () => {
  const source = readSource('src/components/SystemReclaim.vue');

  assert.match(
    source,
    /:disabled="executingId !== null \|\| \(op\.requires_admin && !isElevated\)"/,
  );
});

test('system reclaim confirmation does not promise migration history', () => {
  const source = readSource('src/components/SystemReclaim.vue');

  assert.doesNotMatch(source, /执行后可在「迁移历史」里看到记录/);
  assert.match(source, /执行结果会显示在当前页面，重新扫描后更新空间/);
});

test('junk feedback explains that the report is stored locally', () => {
  const source = readSource('src/components/JunkCleanView.vue');

  assert.doesNotMatch(source, /感谢反馈，下个版本会优化此规则/);
  assert.match(source, /反馈已保存在本机/);
});

test('privacy close button has an explicit type and accessible name', () => {
  const source = readSource('src/components/Privacy.vue');

  assert.match(
    source,
    /<button\s+class="close-btn"\s+type="button"\s+aria-label="关闭隐私政策"\s+@click="emit\('close'\)"/,
  );
});
