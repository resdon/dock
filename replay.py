import json

def apply_replace_file_content(args):
    filepath = args.get("TargetFile")
    target = args.get("TargetContent")
    replacement = args.get("ReplacementContent")
    try:
        with open(filepath, "r") as f:
            content = f.read()
    except FileNotFoundError:
        print(f"File not found: {filepath}")
        return
    
    if target in content:
        content = content.replace(target, replacement, 1)
        with open(filepath, "w") as f:
            f.write(content)
        print(f"Applied replace to {filepath}")
    else:
        print(f"Failed to find target in {filepath}")

def apply_multi_replace(args):
    filepath = args.get("TargetFile")
    chunks = args.get("ReplacementChunks", [])
    try:
        with open(filepath, "r") as f:
            content = f.read()
    except FileNotFoundError:
        print(f"File not found: {filepath}")
        return
    
    for chunk in chunks:
        target = chunk.get("TargetContent")
        replacement = chunk.get("ReplacementContent")
        if target in content:
            content = content.replace(target, replacement, 1)
            print(f"Applied one chunk to {filepath}")
        else:
            print(f"Failed to find a chunk target in {filepath}")
            
    with open(filepath, "w") as f:
        f.write(content)

def apply_write_to_file(args):
    filepath = args.get("TargetFile")
    content = args.get("CodeContent", args.get("content", ""))
    if not filepath:
        return
    # write_to_file uses CodeContent, but check if we have it
    if not content:
        print(f"No content for {filepath}")
        return
    import os
    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    with open(filepath, "w") as f:
        f.write(content)
    print(f"Wrote to {filepath}")

with open("/home/resdon/.gemini/antigravity-ide/brain/89a3f001-6438-4cf0-875d-47994f2647cd/.system_generated/logs/transcript_full.jsonl", "r") as f:
    for line in f:
        try:
            data = json.loads(line)
        except:
            continue
        if data.get("type") == "PLANNER_RESPONSE":
            for call in data.get("tool_calls", []):
                name = call.get("name")
                # Sometimes arguments is a JSON string, sometimes a dict
                args = call.get("arguments", {})
                if isinstance(args, str):
                    try:
                        args = json.loads(args)
                    except:
                        pass
                if name == "default_api:replace_file_content":
                    apply_replace_file_content(args)
                elif name == "default_api:multi_replace_file_content":
                    apply_multi_replace(args)
                elif name == "default_api:write_to_file":
                    apply_write_to_file(args)

