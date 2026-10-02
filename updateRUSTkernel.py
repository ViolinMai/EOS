import os
import re
import shutil

USAGE = """
================================================================================
MAI ADVANCED SYNC & PATCH MANAGER (Zero-Indent Top-Level Engine v3.0)
================================================================================
Supported patch formats in 'patch.txt':

1) Full File Replacement:
--------------------------------------------------
### FILE: path/to/file.rs
<full file contents>
### END

2) Zero-Indent Item Replacement (fn, impl, struct, enum, use, static, const, trait):
- Automatically locates the old top-level definition, balances braces { }, and replaces it safely.
- If it does not exist, it appends it automatically.
--------------------------------------------------
### ITEM: path/to/file.rs
pub fn resolve_path(path: &str) -> String {
    let mut p = path.trim().replace('\\', "/");
    if p.starts_with("./") { p = String::from(&p[2..]); }
    if p.starts_with('/') { p = String::from(&p[1..]); }
    p
}
### END

Instructions:
1. Paste patches into 'patch.txt'.
2. Run: python updateRUSTkernel.py
3. If 'patch.txt' is empty, a project dump will be generated in 'full_project_dump.txt'.
================================================================================
"""

SOURCE_CODE_EXTENSIONS = (
    '.rs', '.py', '.ld', '.conf', '.toml', '.s', '.asm', '.nsh', '.TAG', '.lock', '.ini'
)

EXACT_SOURCE_FILES = {'CMakeLists.txt'}

IGNORED_DIRS = {
    "__pycache__", ".git", "dist", "build", ".venv", "venv",
    "node_modules", ".dart_tool", "ephemeral",
    ".gradle", ".idea", ".vscode", "Pods", ".fvm",
    "data", "downloads", "shared", "debug", "tools", "x86_64-unknown-none"
}

DUMP_FILENAME = "full_project_dump.txt"
PATCH_FILENAME = "patch.txt"

def is_source_file(filename):
    if filename in {DUMP_FILENAME, PATCH_FILENAME, "index_cache.json", "cert.der", "key.der"}:
        return False
    if filename.endswith(".bak"):
        return False
    if filename in EXACT_SOURCE_FILES:
        return True
    return filename.endswith(SOURCE_CODE_EXTENSIONS)

def get_project_data(startpath='.'):
    source_files = []
    tree_lines = [f"PROJECT SOURCE STRUCTURE: {os.path.basename(os.path.abspath(startpath))}", "-" * 80]
    
    for root, dirs, files in os.walk(startpath):
        rel_root = os.path.relpath(root, startpath).replace("\\", "/")
        dirs[:] = [d for d in dirs if d not in IGNORED_DIRS and not d.startswith('.')]
        valid_files = [f for f in sorted(files) if is_source_file(f) and f != os.path.basename(__file__)]
        
        level = 0 if rel_root == '.' else rel_root.count('/') + 1
        folder_name = os.path.basename(os.path.abspath(startpath)) if rel_root == '.' else os.path.basename(root)

        if valid_files or dirs:
            indent = ' ' * 4 * level
            tree_lines.append(f"{indent}[DIR]  {folder_name}/")
            subindent = ' ' * 4 * (level + 1)
            for f in valid_files:
                tree_lines.append(f"{subindent}[FILE] {f}")
                source_files.append(os.path.join(root, f))
                
    return "\n".join(tree_lines), source_files

def write_full_dump(tree_str, source_files, output_file=DUMP_FILENAME):
    with open(output_file, 'w', encoding='utf-8') as dump:
        dump.write(tree_str + "\n\n")
        dump.write("=" * 80 + "\nFULL SOURCE CODE DUMP\n" + "=" * 80 + "\n\n")
        
        for fpath in source_files:
            clean_fpath = fpath.replace("\\", "/")
            dump.write(f"\n{'=' * 80}\n--- FILE: {clean_fpath} ---\n{'=' * 80}\n\n")
            try:
                with open(fpath, 'r', encoding='utf-8', errors='replace') as src:
                    dump.write(src.read().strip() + "\n")
            except Exception as e:
                dump.write(f"[ERROR READING FILE]: {e}\n")
                
    print(f"\n[OK] Source code dump saved to: {output_file}")

def normalize_newlines(text):
    return text.replace('\r\n', '\n').replace('\r', '\n')

def extract_signature_identifier(first_line):
    line = first_line.strip()
    match = re.search(r'\b(fn|struct|enum|trait|impl|static|const|mod|type)\s+([a-zA-Z0-9_]+)', line)
    if match:
        return match.group(1), match.group(2)
    if line.startswith("use "):
        return "use", line
    return None, None

def find_item_boundaries(file_lines, kind, name):
    for i, line in enumerate(file_lines):
        if line.startswith((' ', '\t')):
            continue
        curr_kind, curr_name = extract_signature_identifier(line)
        if curr_kind == kind and curr_name == name:
            start_idx = i
            # If it's a one-line statement without braces
            if ';' in line and '{' not in line:
                return start_idx, start_idx + 1
            
            brace_count = 0
            found_opening = False
            for j in range(start_idx, len(file_lines)):
                l = file_lines[j]
                brace_count += l.count('{') - l.count('}')
                if '{' in l:
                    found_opening = True
                if found_opening and brace_count <= 0:
                    return start_idx, j + 1
                if not found_opening and ';' in l:
                    return start_idx, j + 1
    return None, None

def apply_item_patch(file_content, replacement_item):
    file_lines = normalize_newlines(file_content).split('\n')
    item_lines = [l for l in normalize_newlines(replacement_item).split('\n')]
    
    while item_lines and not item_lines[0].strip():
        item_lines.pop(0)
    while item_lines and not item_lines[-1].strip():
        item_lines.pop()

    if not item_lines:
        return True, file_content, "Empty item skipped."

    kind, name = extract_signature_identifier(item_lines[0])
    if not kind:
        # Fallback: append
        return True, file_content.rstrip() + "\n\n" + '\n'.join(item_lines) + "\n", "Appended definition"

    start, end = find_item_boundaries(file_lines, kind, name)
    if start is not None and end is not None:
        new_lines = file_lines[:start] + item_lines + file_lines[end:]
        return True, '\n'.join(new_lines), f"Replaced top-level item '{name}'"
    else:
        # If item doesn't exist, append it cleanly
        new_content = file_content.rstrip() + "\n\n" + '\n'.join(item_lines) + "\n"
        return True, new_content, f"Appended new top-level item '{name}'"

def apply_patch():
    print(USAGE)
    
    if not os.path.exists(PATCH_FILENAME) or os.path.getsize(PATCH_FILENAME) == 0:
        print("[!] 'patch.txt' is empty. Extracting latest code dump...")
        tree_str, source_files = get_project_data('.')
        print("\n" + tree_str)
        write_full_dump(tree_str, source_files)
        return

    print("[*] Parsing patch and verifying changes...")
    with open(PATCH_FILENAME, 'r', encoding='utf-8') as f:
        raw_patch = f.read()

    lines = raw_patch.splitlines(keepends=True)
    idx = 0
    total_lines = len(lines)
    
    operations_map = {}
    parse_errors = []

    while idx < total_lines:
        line = lines[idx]
        stripped = line.strip()

        if stripped.startswith("### FILE:"):
            target_file = stripped.split("### FILE:")[1].strip()
            idx += 1
            content = []
            while idx < total_lines and not lines[idx].strip().startswith("### END"):
                content.append(lines[idx])
                idx += 1
            if idx >= total_lines:
                parse_errors.append(f"Missing ### END for file: {target_file}")
                break
            operations_map.setdefault(target_file, []).append({
                'type': 'FILE',
                'content': "".join(content)
            })

        elif stripped.startswith("### ITEM:"):
            target_file = stripped.split("### ITEM:")[1].strip()
            idx += 1
            content = []
            while idx < total_lines and not lines[idx].strip().startswith("### END"):
                content.append(lines[idx])
                idx += 1
            if idx >= total_lines:
                parse_errors.append(f"Missing ### END for item: {target_file}")
                break
            operations_map.setdefault(target_file, []).append({
                'type': 'ITEM',
                'content': "".join(content)
            })

        idx += 1

    if parse_errors:
        print("\n[ERR] Patch parsing failed:")
        for err in parse_errors:
            print(f"  - {err}")
        return

    staged_files = {}

    for target_rel, ops in operations_map.items():
        target_path = os.path.normpath(target_rel)
        file_exists = os.path.exists(target_path)

        if not file_exists and any(op['type'] == 'ITEM' for op in ops):
            print(f"\n[ERR] Target file '{target_rel}' does not exist for item modification.")
            return

        current_content = ""
        if file_exists:
            with open(target_path, 'r', encoding='utf-8', errors='replace') as f:
                current_content = f.read()

        working_content = current_content

        for op in ops:
            if op['type'] == 'FILE':
                working_content = op['content']
            elif op['type'] == 'ITEM':
                ok, res_text, msg = apply_item_patch(working_content, op['content'])
                if not ok:
                    print(f"\n[ERR] Item patch failed on '{target_rel}': {msg}")
                    return
                working_content = res_text

        staged_files[target_path] = (target_rel, working_content, file_exists)

    for target_path, (target_rel, content, file_exists) in staged_files.items():
        os.makedirs(os.path.dirname(target_path) or '.', exist_ok=True)
        if file_exists:
            shutil.copyfile(target_path, target_path + ".bak")

        with open(target_path, 'w', encoding='utf-8') as out_f:
            out_f.write(content)
        print(f"  [+] Patched: {target_rel}")

    with open(PATCH_FILENAME, 'w', encoding='utf-8') as f:
        f.write("")
    print(f"\n[OK] Successfully applied patch to {len(staged_files)} file(s).")

    tree_str, source_files = get_project_data('.')
    write_full_dump(tree_str, source_files)

if __name__ == "__main__":
    apply_patch()