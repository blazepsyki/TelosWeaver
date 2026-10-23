// 저장소 루트의 schemas/*.schema.json(Rust telos-schema에서 생성)으로 TypeScript 타입을 만듭니다 (Spec §3.4).
// 생성물(src/generated/)은 커밋하지 않으며 손으로 수정하지 않습니다.

import { mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile } from 'json-schema-to-typescript';

const schemasDir = fileURLToPath(new URL('../../schemas/', import.meta.url));
const outDir = fileURLToPath(new URL('../src/generated/', import.meta.url));

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });

const names = [];
for (const file of (await readdir(schemasDir)).filter((f) => f.endsWith('.schema.json')).sort()) {
  const name = basename(file, '.schema.json');
  const schema = JSON.parse(await readFile(join(schemasDir, file), 'utf8'));
  const ts = await compile(schema, name, {
    bannerComment: `/* 자동 생성: schemas/${file}. 직접 수정하지 마세요. */`,
    additionalProperties: false,
  });
  await writeFile(join(outDir, `${name}.ts`), ts);
  names.push(name);
}

console.log(`generated ${names.length} files: ${names.join(', ')}`);
