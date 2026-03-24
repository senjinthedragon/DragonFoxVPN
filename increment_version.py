# increment_version.py
"""
increment_version.py - DragonFoxVPN: Build number increment helper
Copyright (c) 2026 Senjin the Dragon.
https://github.com/senjinthedragon/DragonFoxVPN
Licensed under the MIT License.
See LICENSE for full license information.

Increments the fourth version component (build number) in version_info.txt
and syncs the new version string into dragonfox_vpn.py. Called automatically
by build_windows.ps1 before each PyInstaller build.
"""

import re
import sys
from pathlib import Path

VERSION_FILE = Path("version_info.txt")
APP_FILE = Path("dragonfox_vpn.py")

def increment_version(version_str):
    # Expects 1.0.0 or 1.0.0.0
    parts = [int(x) for x in version_str.split('.')]
    # Ensure we have 4 parts
    while len(parts) < 4:
        parts.append(0)
    
    # Increment 4th digit (Build Number)
    parts[3] += 1 
    
    return ".".join(map(str, parts))

def update_version_info(new_version):
    if not VERSION_FILE.exists():
        print("version_info.txt not found!")
        return

    content = VERSION_FILE.read_text(encoding='utf-8')
    
    # Update filevers=(1, 0, 0, 0)
    # Parse new version into tuple
    v_parts = [int(x) for x in new_version.split('.')]
    while len(v_parts) < 4: v_parts.append(0)
    v_tuple = f"({', '.join(map(str, v_parts))})"
    
    content = re.sub(r'filevers=\([\d,\s]+\)', f'filevers={v_tuple}', content)
    content = re.sub(r'prodvers=\([\d,\s]+\)', f'prodvers={v_tuple}', content)
    
    # Update StringStruct(u'FileVersion', u'1.0.0.0')
    content = re.sub(r"StringStruct\(u'FileVersion', u'[\d\.]+'\)", f"StringStruct(u'FileVersion', u'{new_version}')", content)
    content = re.sub(r"StringStruct\(u'ProductVersion', u'[\d\.]+'\)", f"StringStruct(u'ProductVersion', u'{new_version}')", content)
    
    VERSION_FILE.write_text(content, encoding='utf-8')
    print(f"Updated version_info.txt to {new_version}")

def update_app_file(new_version):
    if not APP_FILE.exists():
        return
        
    content = APP_FILE.read_text(encoding='utf-8')
    
    # Look for __version__ = "..."
    if '__version__' in content:
        content = re.sub(r'__version__\s*=\s*["\'][\d\.]+["\']', f'__version__ = "{new_version}"', content)
    else:
        # Insert after docstring or imports
        # Find the end of the docstring """ ... """
        match = re.search(r'"""[\s\S]*?"""', content)
        if match:
            end_pos = match.end()
            content = content[:end_pos] + f'\n\n__version__ = "{new_version}"' + content[end_pos:]
        else:
            content = f'__version__ = "{new_version}"\n' + content
            
    APP_FILE.write_text(content, encoding='utf-8')
    print(f"Updated dragonfox_vpn.py to {new_version}")

def main():
    # Read current version from file (or default)
    # We use version_info.txt as source of truth
    content = VERSION_FILE.read_text(encoding='utf-8')
    match = re.search(r"StringStruct\(u'FileVersion', u'([\d\.]+)'\)", content)
    if match:
        current_version = match.group(1)
    else:
        current_version = "1.0.0.0"
        
    new_version = increment_version(current_version)
    print(f"Incrementing version: {current_version} -> {new_version}")
    
    update_version_info(new_version)
    update_app_file(new_version)

if __name__ == "__main__":
    main()
