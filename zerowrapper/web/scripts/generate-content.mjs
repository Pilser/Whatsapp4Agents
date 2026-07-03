import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import yaml from 'js-yaml';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');
const DATA = join(ROOT, 'src', 'data');

function loadYaml(name) {
  return yaml.load(readFileSync(join(ROOT, name), 'utf-8'));
}

function writeJson(name, data) {
  mkdirSync(DATA, { recursive: true });
  writeFileSync(join(DATA, name), JSON.stringify(data, null, 2));
}

writeJson('sitesettings.json', loadYaml('sitesettings.yaml'));
writeJson('landing.json', loadYaml('landing.yaml'));

console.log('Generated src/data/sitesettings.json');
console.log('Generated src/data/landing.json');
