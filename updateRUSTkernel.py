import os

USAGE = """
================================================================================
🛠️ MAI SYNC & PATCH MANAGER (EOS Kernel Edition) 🛠️
================================================================================
Instructions:
1. Paste patches into 'patch.txt'.
2. Run: python updateRUST.py
3. The full structure & clean code are saved to 'full_project_dump.txt'
================================================================================
"""

# صيغ ملفات الأكواد، السكريبتات، وملفات التهيئة الخاصة بالنواة
SOURCE_CODE_EXTENSIONS = (
    '.rs', '.py', '.ld', '.conf', '.toml', '.s', '.asm', '.nsh', '.TAG'
)

EXACT_SOURCE_FILES = {'CMakeLists.txt'}

# المجلدات المستبعدة كلياً (مجلد البناء target ومجلدات النظام والتخزين المؤقت)
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
    if filename in EXACT_SOURCE_FILES:
        return True
    return filename.endswith(SOURCE_CODE_EXTENSIONS)

def get_project_data(startpath='.'):
    source_files = []
    tree_lines = [f"📂 PROJECT SOURCE STRUCTURE: {os.path.basename(os.path.abspath(startpath))}", "-" * 80]
    
    for root, dirs, files in os.walk(startpath):
        rel_root = os.path.relpath(root, startpath).replace("\\", "/")
        
        # استبعاد المجلدات غير المرغوبة
        dirs[:] = [
            d for d in dirs 
            if d not in IGNORED_DIRS 
            and not d.startswith('.')
        ]
        
        valid_files = [f for f in sorted(files) if is_source_file(f) and f != os.path.basename(__file__)]
        
        if rel_root == '.':
            level = 0
            folder_name = os.path.basename(os.path.abspath(startpath))
        else:
            level = rel_root.count('/') + 1
            folder_name = os.path.basename(root)

        if valid_files or dirs:
            indent = ' ' * 4 * level
            tree_lines.append(f"{indent}📁 {folder_name}/")
            subindent = ' ' * 4 * (level + 1)
            for f in valid_files:
                tree_lines.append(f"{subindent}📄 {f}")
                source_files.append(os.path.join(root, f))
                
    return "\n".join(tree_lines), source_files

def write_full_dump(tree_str, source_files, output_file=DUMP_FILENAME):
    with open(output_file, 'w', encoding='utf-8') as dump:
        dump.write(tree_str + "\n\n")
        dump.write("=" * 80 + "\n💻 FULL SOURCE CODE DUMP\n" + "=" * 80 + "\n\n")
        
        for fpath in source_files:
            if os.path.basename(fpath) == DUMP_FILENAME:
                continue
                
            clean_fpath = fpath.replace("\\", "/")
            dump.write(f"\n{'=' * 80}\n--- FILE: {clean_fpath} ---\n{'=' * 80}\n\n")
            try:
                with open(fpath, 'r', encoding='utf-8', errors='replace') as src:
                    dump.write(src.read().strip() + "\n")
            except Exception as e:
                dump.write(f"[ERROR READING FILE]: {e}\n")
                
    print(f"\n[✓] Source code structure and content successfully saved to: {output_file}")

def apply_patch():
    print(USAGE)
    
    if os.path.exists(PATCH_FILENAME) and os.path.getsize(PATCH_FILENAME) > 0:
        print("[*] Processing patch...")
        with open(PATCH_FILENAME, 'r', encoding='utf-8') as f:
            lines = f.readlines()

        current_file = None
        file_content = []
        updated_files = []

        for line in lines:
            if line.startswith("### FILE:"):
                current_file = line.strip().split("### FILE:")[1].strip()
                file_content = []
            elif line.startswith("### END") and current_file:
                target_path = os.path.normpath(current_file)
                os.makedirs(os.path.dirname(target_path) or '.', exist_ok=True)
                
                with open(target_path, 'w', encoding='utf-8') as out_f:
                    out_f.writelines(file_content)
                    
                updated_files.append(current_file)
                print(f"  [+] Updated: {current_file}")
                current_file = None
            elif current_file:
                file_content.append(line)

        if updated_files:
            with open(PATCH_FILENAME, 'w', encoding='utf-8') as f:
                f.write("")
            print(f"[✓] Applied patch to {len(updated_files)} file(s).")
    else:
        print("[!] 'patch.txt' is empty. Extracting latest code dump.")

    tree_str, source_files = get_project_data('.')
    print("\n" + tree_str)
    write_full_dump(tree_str, source_files)

if __name__ == "__main__":
    apply_patch()