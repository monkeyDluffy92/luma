import os
import subprocess
from PIL import Image, ImageDraw

def finalize_icons_v5():
    # Goal: Standard macOS Squircle.
    # The previous attempt (v4) was a sharp square (100% white).
    # The attempt before (v3) was a squircle but user said "Big" (maybe optical).
    # The attempt before (shrink) was "Small".
    
    # Correct path: Match Docker/Ollama structure.
    # They are "Full Bleed Squircles". They have transparent corners.
    # We will generate exactly that.
    
    current_path = '/Users/aamirkhan/luma/src-tauri/icons/icon.png'
    if not os.path.exists(current_path):
        print("Error: Icon not found")
        return
        
    img = Image.open(current_path).convert('RGBA')
    
    # 1. Recover the Cat and White Background logic
    # Since current icon is a full white square (from v4), we can just mask it.
    
    # Create Mask (Squircle)
    mask = Image.new('L', (1024, 1024), 0)
    draw_mask = ImageDraw.Draw(mask)
    draw_mask.rounded_rectangle([(0,0), (1024,1024)], radius=224, fill=255)
    
    # Create Final Image
    final = Image.new('RGBA', (1024, 1024), (0, 0, 0, 0))
    
    # Paste the current full-square icon using the mask
    final.paste(img, (0, 0), mask=mask)
    
    # Save
    final.save(current_path)
    print("Icon masked to Squircle (Standard macOS Shape).")
    
    # Regenerate ICNS
    iconset_dir = '/Users/aamirkhan/luma/src-tauri/icons/icon.iconset'
        
    sizes = [
        (16, 16, 'icon_16x16.png'),
        (32, 32, 'icon_16x16@2x.png'),
        (32, 32, 'icon_32x32.png'),
        (64, 64, 'icon_32x32@2x.png'),
        (128, 128, 'icon_128x128.png'),
        (256, 256, 'icon_128x128@2x.png'),
        (256, 256, 'icon_256x256.png'),
        (512, 512, 'icon_256x256@2x.png'),
        (512, 512, 'icon_512x512.png'),
        (1024, 1024, 'icon_512x512@2x.png')
    ]
    
    for w, h, name in sizes:
        r = final.resize((w, h), Image.Resampling.LANCZOS)
        r.save(os.path.join(iconset_dir, name))
        
    cmd = ['iconutil', '-c', 'icns', iconset_dir, '-o', '/Users/aamirkhan/luma/src-tauri/icons/icon.icns']
    subprocess.run(cmd, check=True)
    print("ICNS regenerated.")

if __name__ == "__main__":
    finalize_icons_v5()
