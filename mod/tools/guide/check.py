#!/usr/bin/env python3
"""Checks that the in-game documentation is the same in both languages.

    python3 mod/tools/guide/check.py [repo root]

 - assets/fcw_docs/lang: en_us.json and ru_ru.json have the same keys;
 - assets/fcw/fcwguide: every English page has a Russian one under _ru_ru and the other way round,
   with the same tags (scenes, recipes, tables of ports) in the same order;
 - every structure a page imports exists; every link to another page resolves.

Exit code 1 if anything is off. See docs/GUIDE.md.
"""
import json
import os
import re
import sys

root = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), '..', '..', '..')
assets = os.path.join(root, 'mod', 'src', 'main', 'resources', 'assets')
guide = os.path.join(assets, 'fcw', 'fcwguide')
problems = []

# ---------------------------------------------------------------- lang
lang = {}
for name in ('en_us', 'ru_ru'):
    with open(os.path.join(assets, 'fcw_docs', 'lang', name + '.json'), encoding='utf-8') as f:
        lang[name] = json.load(f)
for key in sorted(set(lang['en_us']) ^ set(lang['ru_ru'])):
    problems.append(f'lang: {key} is missing in {"ru_ru" if key in lang["en_us"] else "en_us"}')


# ---------------------------------------------------------------- pages
def pages(base):
    found = {}
    for folder, dirs, files in os.walk(base):
        dirs[:] = [d for d in dirs if not d.startswith('_') and d != 'assets']
        for file in files:
            if file.endswith('.md'):
                path = os.path.join(folder, file)
                found[os.path.relpath(path, base).replace(os.sep, '/')] = open(path, encoding='utf-8').read()
    return found


en, ru = pages(guide), pages(os.path.join(guide, '_ru_ru'))
for page in sorted(set(en) ^ set(ru)):
    problems.append(f'page: {page} is missing in {"Russian" if page in en else "English"}')

TAG = re.compile(r'<(GameScene|ImportStructure|MachinePorts|MachinePortTable|MachineRecipes|Recipe|RecipeFor|SubPages|IsometricCamera)\b[^>]*>')
LINK = re.compile(r'\]\(([^)#]+\.md)(#[^)]*)?\)')
SRC = re.compile(r'<ImportStructure src="([^"]+)"')
PARENT = re.compile(r'^\s*parent:\s*(\S+)\s*$', re.M)

for page in sorted(set(en) & set(ru)):
    if TAG.findall(en[page]) != TAG.findall(ru[page]) or [m.group(0) for m in TAG.finditer(en[page])] != [m.group(0) for m in TAG.finditer(ru[page])]:
        problems.append(f'page: {page} has different tags in the two languages')

for label, set_ in (('en', en), ('ru', ru)):
    for page, text in set_.items():
        here = os.path.dirname(page)
        for src in SRC.findall(text):
            if not os.path.exists(os.path.normpath(os.path.join(guide, here, src))):
                problems.append(f'{label} {page}: structure {src} does not exist')
        for link, _ in LINK.findall(text):
            if os.path.normpath(os.path.join(here, link)).replace(os.sep, '/') not in set_:
                problems.append(f'{label} {page}: link to {link} leads nowhere')
        for parent in PARENT.findall(text):
            if parent not in set_:
                problems.append(f'{label} {page}: navigation parent {parent} does not exist')

for line in problems:
    print(line)
print(f'{len(en)} pages, {len(lang["en_us"])} lang keys, {len(problems)} problem(s)')
sys.exit(1 if problems else 0)
