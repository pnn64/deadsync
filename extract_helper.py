import sys, re

def read_file(path):
    with open(path, 'r') as f:
        return f.readlines()

def write_file(path, lines):
    with open(path, 'w') as f:
        f.writelines(lines)

def find_const_block(lines, name):
    """Find start/end (0-indexed) of a pub const NAME: &[...] = &[...]; block"""
    start = None
    for i, line in enumerate(lines):
        if f'pub const {name}:' in line:
            start = i
            break
    if start is None:
        return None, None
    # Find matching ];
    depth = 0
    for i in range(start, len(lines)):
        for ch in lines[i]:
            if ch == '[':
                depth += 1
            elif ch == ']':
                depth -= 1
        if lines[i].strip() == '];':
            return start, i
    return start, None

def extract_blocks(mod_path, target_path, const_names, extra_line_ranges=None):
    """Extract named const blocks (and optional extra line ranges) from mod.rs to target file.
    extra_line_ranges: list of (start, end) 1-indexed inclusive ranges of lines to also extract.
    """
    lines = read_file(mod_path)
    
    # Collect all ranges to extract (0-indexed inclusive)
    ranges = []
    for name in const_names:
        start, end = find_const_block(lines, name)
        if start is None:
            print(f"ERROR: Could not find {name}")
            sys.exit(1)
        ranges.append((start, end))
        print(f"  {name}: lines {start+1}-{end+1}")
    
    if extra_line_ranges:
        for s, e in extra_line_ranges:
            ranges.append((s-1, e-1))  # convert to 0-indexed
            print(f"  Extra range: lines {s}-{e}")
    
    # Sort ranges by start, merge overlapping
    ranges.sort()
    merged = [ranges[0]]
    for s, e in ranges[1:]:
        if s <= merged[-1][1] + 2:  # allow 1 blank line gap
            merged[-1] = (merged[-1][0], max(merged[-1][1], e))
        else:
            merged.append((s, e))
    
    # Extract blocks (collect content, then remove from end to preserve indices)
    extracted_blocks = []
    for s, e in merged:
        # Include leading blank line if there's a trailing blank after previous content
        block = lines[s:e+1]
        extracted_blocks.append(block)
    
    # Remove from lines (reverse order to preserve indices)
    for s, e in reversed(merged):
        # Also remove trailing blank line if present
        end_rm = e + 1
        if end_rm < len(lines) and lines[end_rm].strip() == '':
            end_rm += 1
        lines[s:end_rm] = []
    
    # Fix visibility in extracted content
    vis = 'pub(in crate::screens::options)'
    all_extracted = []
    for block in extracted_blocks:
        for line in block:
            line = re.sub(r'^pub const ', f'{vis} const ', line)
            line = re.sub(r'^pub fn ', f'{vis} fn ', line)
            line = re.sub(r'^pub struct ', f'{vis} struct ', line)
            line = re.sub(r'^pub enum ', f'{vis} enum ', line)
            all_extracted.append(line)
        all_extracted.append('\n')  # blank line between blocks
    
    # Write target file
    target_content = 'use super::super::*;\n\n' + ''.join(all_extracted)
    write_file(target_path, target_content)
    
    # Write back mod.rs
    write_file(mod_path, lines)
    
    total_extracted = sum(e - s + 1 for s, e in merged)
    print(f"  Extracted ~{total_extracted} lines, mod.rs now has {len(lines)} lines")

if __name__ == '__main__':
    # Usage: python3 extract_helper.py <target_file> <const1> <const2> ...
    target = sys.argv[1]
    consts = sys.argv[2:]
    mod_path = 'src/screens/options/mod.rs'
    target_path = f'src/screens/options/submenus/{target}'
    extract_blocks(mod_path, target_path, consts)
