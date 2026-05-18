import { readFileSync, writeFileSync } from 'fs';
const f = 'src-tauri/rules/explain_rules.json';
const rules = JSON.parse(readFileSync(f, 'utf-8'));
const add = JSON.parse(readFileSync('scripts/new_rules.json', 'utf-8'));
rules.push(...add);
writeFileSync(f, JSON.stringify(rules, null, 2), 'utf-8');
console.log(`Total: ${rules.length} rules`);
