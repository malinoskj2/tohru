"""Bake a clean, on-model 2D UV texture onto a sculpted 3D dragon mesh from Sheet B's views.

python3 b1_bake.py SHAPE.glb SHEET_DIR OUT_DIR [--depth-scale 1.0] [--tex-size 1024]

Pipeline:
1. Normalize SHAPE.glb to render units (height 0.96, centered at origin) and UV-unwrap with xatlas.
2. Render 4 orthogonal 16-bit position/alpha views (front=0, profile-right=90, profile-left=-90, back=180)
   plus 16-bit UV-space object-position and object-normal maps in Blender.
3. Strip 2D silhouette ink outlines from each Sheet B view (erode + edge_fill) so view-dependent outlines
   never bake as seams onto the belly or cheeks; remove eyes/mouth from side profiles so face marks come
   strictly once from front.png (eliminating far-eye winks and doubled marks).
4. Align each Sheet B view onto its rendered silhouette, project visible front-surface pixels into UV space
   weighted by surface facing and view priority, and smoothly fill any occluded crevices.
5. Export albedo.png and textured.glb ready for render_turn.py.
"""
import argparse
import json
import math
import subprocess
import sys
from pathlib import Path

import cv2
import numpy as np
from PIL import Image, ImageOps
from scipy import ndimage
import trimesh
import xatlas

ORTHO = 1.25
HERE = Path(__file__).resolve().parent


def edge_fill(rgb, mask, iterations=40):
    out = rgb.astype(np.float32).copy()
    m = mask.copy()
    out[~m] = 0.0
    for _ in range(iterations):
        blur = cv2.blur(out, (3, 3))
        wts = cv2.blur(m.astype(np.float32), (3, 3))
        grow = (~m) & (wts > 1e-4)
        if not grow.any():
            break
        out[grow] = blur[grow] / wts[grow, None]
        m = m | grow
    return np.clip(out, 0, 255).astype(np.uint8)


def strip_outline(rgba, erode_px=4):
    """Remove the 2D silhouette contour ink line and extend interior pastel colors outward."""
    a = rgba[..., 3] > 127
    if erode_px > 0:
        kernel = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * erode_px + 1, 2 * erode_px + 1))
        inner = cv2.erode(a.astype(np.uint8), kernel) > 0
    else:
        inner = a
    # Protect narrow tips (crest/horns) from being completely erased to black
    if inner.sum() < 100:
        inner = a
    return edge_fill(rgba[..., :3], inner, iterations=96), a


def remove_side_face_marks(rgb, alpha):
    """Inpaint dark eye/mouth marks and inner tail-spiral crease on side profiles so eyes/mouth bake
    strictly from front.png and no 2D tail spiral prints onto the side of the belly."""
    ys, xs = np.nonzero(alpha)
    if not len(xs):
        return rgb
    y0, y1 = ys.min(), ys.max()
    h = y1 - y0
    lum = rgb.astype(np.float32) @ np.array([0.299, 0.587, 0.114], np.float32) / 255.0
    # 1. Dark eye/mouth marks in head band (18%..50% of height)
    dark = (lum < 0.62) & alpha
    dark[int(y0 + 0.50 * h):] = False
    dark[:int(y0 + 0.18 * h)] = False
    out = rgb.copy()
    if dark.sum() > 0:
        dil = cv2.dilate(dark.astype(np.uint8) * 255, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (13, 13)))
        out = cv2.inpaint(out, dil, 9, cv2.INPAINT_TELEA)
    # 2. Inner tail-curl hole + dark spiral shadow in lower-back quadrant (prevents printing a swirl onto the flank)
    filled = ndimage.binary_fill_holes(alpha)
    hole = filled & ~alpha
    if hole.sum() > 0:
        hdil = cv2.dilate(hole.astype(np.uint8) * 255, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (55, 55)))
        inpainted = cv2.inpaint(out, hdil, 21, cv2.INPAINT_TELEA)
        # Smoothly feather the inpainted region with a soft blur so no circular patch shows on the flank
        blur = cv2.GaussianBlur(inpainted, (41, 41), 0)
        soft_m = cv2.GaussianBlur((hdil > 0).astype(np.float32), (31, 31), 0)[..., None]
        out = np.clip(out * (1.0 - soft_m) + blur * soft_m, 0, 255).astype(np.uint8)
    return out


def align_view_to_mask(view_rgb, view_alpha, target_alpha, allow_aniso=True):
    """Align a Sheet B view onto the rendered 3D silhouette mask via bbox fit (returning aligned RGB + IoU)."""
    th, tw = target_alpha.shape
    ys_s, xs_s = np.nonzero(view_alpha)
    ys_t, xs_t = np.nonzero(target_alpha)
    sx0, sx1, sy0, sy1 = xs_s.min(), xs_s.max(), ys_s.min(), ys_s.max()
    tx0, tx1, ty0, ty1 = xs_t.min(), xs_t.max(), ys_t.min(), ys_t.max()
    sw, sh = max(1, sx1 - sx0), max(1, sy1 - sy0)
    tw_b, th_b = max(1, tx1 - tx0), max(1, ty1 - ty0)
    if allow_aniso:
        kx = tw_b / sw
        ky = th_b / sh
    else:
        kx = ky = th_b / sh
    scx, scy = (sx0 + sx1) / 2.0, (sy0 + sy1) / 2.0
    tcx, tcy = (tx0 + tx1) / 2.0, (ty0 + ty1) / 2.0
    M = np.float32([[kx, 0, tcx - scx * kx], [0, ky, tcy - scy * ky]])
    reg_rgb = cv2.warpAffine(view_rgb, M, (tw, th), flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
    reg_a = cv2.warpAffine(view_alpha.astype(np.uint8) * 255, M, (tw, th), flags=cv2.INTER_NEAREST) > 127
    iou = float((reg_a & target_alpha).sum() / max(1, (reg_a | target_alpha).sum()))
    return reg_rgb, reg_a, iou


def read16(path):
    im = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
    if im is None:
        raise FileNotFoundError(path)
    return im[..., [2, 1, 0, 3]].astype(np.float32) / 65535.0


BLENDER_BAKE_SCRIPT = r'''
import math, os, sys
import bpy
import numpy as np

argv = sys.argv[sys.argv.index("--") + 1:]
mesh_path, out_dir, view_size, tex_size = argv[0], argv[1], int(argv[2]), int(argv[3])
os.makedirs(out_dir, exist_ok=True)

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=mesh_path)
meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
bpy.ops.object.select_all(action="DESELECT")
for o in meshes:
    o.select_set(True)
bpy.context.view_layer.objects.active = meshes[0]
if len(meshes) > 1:
    bpy.ops.object.join()
obj = bpy.context.view_layer.objects.active
bpy.ops.object.parent_clear(type="CLEAR_KEEP_TRANSFORM")
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
bpy.ops.object.shade_smooth()

# 1. Render 4 orthogonal 16-bit position passes for visibility/alignment
pivot = bpy.data.objects.new("pivot", None)
bpy.context.scene.collection.objects.link(pivot)
obj.parent = pivot

scene = bpy.context.scene
scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = scene.render.resolution_y = view_size
scene.render.film_transparent = True
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGBA"
scene.render.image_settings.color_depth = "16"
scene.view_settings.view_transform = "Raw"
scene.render.filter_size = 1.0

cam_data = bpy.data.cameras.new("cam")
cam_data.type = "ORTHO"
cam_data.ortho_scale = 1.25
cam = bpy.data.objects.new("cam", cam_data)
scene.collection.objects.link(cam)
cam.location = (0, -5, 0)
cam.rotation_euler = (math.radians(90), 0, 0)
scene.camera = cam

mat_pos = bpy.data.materials.new("pos")
mat_pos.use_nodes = True
nt = mat_pos.node_tree
for n in list(nt.nodes): nt.nodes.remove(n)
out = nt.nodes.new("ShaderNodeOutputMaterial")
em = nt.nodes.new("ShaderNodeEmission")
tc = nt.nodes.new("ShaderNodeTexCoord")
mp = nt.nodes.new("ShaderNodeMapping")
mp.inputs["Location"].default_value = (0.5, 0.5, 0.5)
nt.links.new(tc.outputs["Object"], mp.inputs[0])
nt.links.new(mp.outputs[0], em.inputs[0])
nt.links.new(em.outputs[0], out.inputs[0])
obj.data.materials.clear()
obj.data.materials.append(mat_pos)

for yaw in (0, 90, -90, 180):
    pivot.rotation_euler = (0, 0, math.radians(yaw))
    scene.render.filepath = os.path.join(out_dir, f"view_{yaw}_pos.png")
    bpy.ops.render.render(write_still=True)

# 2. Build a UV-flat mesh with per-corner 3D position and smooth vertex normal, render at tex_size x tex_size
me = obj.data
uv_layer = me.uv_layers.active.data
verts = me.vertices
# Compute smooth vertex normals
pos_list = []
nrm_list = []
uv_verts = []
uv_faces = []
idx = 0
for poly in me.polygons:
    f_idx = []
    for li in poly.loop_indices:
        vi = me.loops[li].vertex_index
        uv = uv_layer[li].uv
        co = verts[vi].co
        no = verts[vi].normal
        uv_verts.append((uv.x, uv.y, 0.0))
        pos_list.append((co.x + 0.5, co.y + 0.5, co.z + 0.5, 1.0))
        nrm_list.append((no.x * 0.5 + 0.5, no.y * 0.5 + 0.5, no.z * 0.5 + 0.5, 1.0))
        f_idx.append(idx)
        idx += 1
    uv_faces.append(f_idx)

bpy.ops.wm.read_factory_settings(use_empty=True)
uv_me = bpy.data.meshes.new("uv_flat")
uv_me.from_pydata(uv_verts, [], uv_faces)
uv_me.update()

def set_attr(name, data):
    attr = uv_me.color_attributes.new(name=name, type="FLOAT_COLOR", domain="POINT")
    flat = np.array(data, dtype=np.float32).ravel()
    attr.data.foreach_set("color", flat)

set_attr("obj_pos", pos_list)
set_attr("obj_nrm", nrm_list)

uv_obj = bpy.data.objects.new("uv_flat", uv_me)
bpy.context.scene.collection.objects.link(uv_obj)

scene = bpy.context.scene
scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = scene.render.resolution_y = tex_size
scene.render.film_transparent = True
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGBA"
scene.render.image_settings.color_depth = "16"
scene.view_settings.view_transform = "Raw"
scene.render.filter_size = 0.01

cam_data = bpy.data.cameras.new("uv_cam")
cam_data.type = "ORTHO"
cam_data.ortho_scale = 1.0
cam = bpy.data.objects.new("uv_cam", cam_data)
scene.collection.objects.link(cam)
cam.location = (0.5, 0.5, 2.0)
cam.rotation_euler = (0, 0, 0)
scene.camera = cam

for attr_name, fname in (("obj_pos", "uv_pos.png"), ("obj_nrm", "uv_nrm.png")):
    mat = bpy.data.materials.new(attr_name)
    mat.use_nodes = True
    nt = mat.node_tree
    for n in list(nt.nodes): nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    at = nt.nodes.new("ShaderNodeAttribute")
    at.attribute_name = attr_name
    nt.links.new(at.outputs["Color"], em.inputs[0])
    nt.links.new(em.outputs[0], out.inputs[0])
    uv_obj.data.materials.clear()
    uv_obj.data.materials.append(mat)
    scene.render.filepath = os.path.join(out_dir, fname)
    bpy.ops.render.render(write_still=True)
'''


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("shape")
    ap.add_argument("sheet")
    ap.add_argument("out")
    ap.add_argument("--depth-scale", type=float, default=1.0)
    ap.add_argument("--tex-size", type=int, default=1024)
    ap.add_argument("--view-size", type=int, default=512)
    ap.add_argument("--tol", type=float, default=0.03)
    ap.add_argument("--power", type=float, default=3.5)
    ap.add_argument("--erode", type=int, default=4)
    ap.add_argument("--smooth-sides", type=int, default=21, help="Gaussian kernel to airbrush side/back views so 2D frill/wing lines do not smear across 3D cheeks")
    args = ap.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    sheet = Path(args.sheet)

    # 1. Load, normalize, depth-scale, and UV-unwrap
    mesh = trimesh.load(args.shape, force="mesh")
    v = mesh.vertices.copy()
    v -= (v.min(0) + v.max(0)) / 2.0
    v[:, 2] *= args.depth_scale
    h_ext = v[:, 1].max() - v[:, 1].min()
    v = v / h_ext * 0.96
    mesh.vertices = v

    vmapping, indices, uvs = xatlas.parametrize(mesh.vertices.astype(np.float32), mesh.faces.astype(np.uint32))
    uv_mesh = trimesh.Trimesh(
        vertices=mesh.vertices[vmapping],
        faces=indices,
        process=False,
        visual=trimesh.visual.TextureVisuals(uv=uvs),
    )
    uv_glb = out / "mesh_uv.glb"
    uv_mesh.export(uv_glb)

    # 2. Run Blender to render orthogonal position views + UV position/normal maps
    bake_py = out / "_bake_passes.py"
    bake_py.write_text(BLENDER_BAKE_SCRIPT)
    passes = out / "bake_passes"
    subprocess.run(
        ["blender", "-b", "-P", str(bake_py), "--", str(uv_glb), str(passes), str(args.view_size), str(args.tex_size)],
        check=True,
        stdout=subprocess.DEVNULL,
    )

    # 3. Load UV maps
    uv_pos = read16(passes / "uv_pos.png")
    uv_nrm = read16(passes / "uv_nrm.png")
    uv_valid = uv_pos[..., 3] > 0.5
    X, Y, Z = uv_pos[..., 0] - 0.5, uv_pos[..., 1] - 0.5, uv_pos[..., 2] - 0.5
    N = uv_nrm[..., :3] * 2.0 - 1.0
    nlen = np.linalg.norm(N, axis=-1, keepdims=True)
    N = N / np.maximum(nlen, 1e-6)

    # 4. Prepare the 4 Sheet B reference views
    view_specs = [
        (0, "front", False, 1.8),
        (90, "profile-right", False, 1.0),
        (-90, "profile-right", True, 1.0),
        (180, "back", False, 1.0),
    ]
    n_v = args.view_size
    ppu = n_v / ORTHO

    acc = np.zeros((args.tex_size, args.tex_size, 3), np.float32)
    wsum = np.zeros((args.tex_size, args.tex_size), np.float32)
    fallback = np.zeros((args.tex_size, args.tex_size, 3), np.float32)
    fbest = np.full((args.tex_size, args.tex_size), -1e9, np.float32)
    ious = {}

    for yaw, sname, mirror, priority in view_specs:
        im = Image.open(sheet / f"{sname}.png").convert("RGBA")
        if mirror:
            im = ImageOps.mirror(im)
        rgba = np.asarray(im)
        s_alpha = rgba[..., 3] > 127
        rgb_src = rgba[..., :3]
        if sname == "profile-right":
            rgb_src = remove_side_face_marks(rgb_src, s_alpha)
        clean_rgb, s_alpha = strip_outline(np.dstack([rgb_src, rgba[..., 3]]), erode_px=args.erode)
        if sname != "front" and args.smooth_sides >= 3:
            k = args.smooth_sides | 1
            clean_rgb = cv2.GaussianBlur(clean_rgb, (k, k), 0)

        vpos = read16(passes / f"view_{yaw}_pos.png")
        valpha = vpos[..., 3] > 0.5
        reg_rgb, reg_a, iou = align_view_to_mask(clean_rgb, s_alpha, valpha, allow_aniso=True)
        ious[f"{yaw}_{sname}{'_m' if mirror else ''}"] = round(iou, 4)
        # Save aligned debug view
        Image.fromarray(reg_rgb).save(passes / f"aligned_{yaw}.png")

        t = math.radians(yaw)
        sx = X * math.cos(t) - Y * math.sin(t)
        depth = X * math.sin(t) + Y * math.cos(t)
        kx = (n_v / 2.0 + sx * ppu).astype(np.float32)
        ky = (n_v / 2.0 - Z * ppu).astype(np.float32)

        col = cv2.remap(reg_rgb.astype(np.float32), kx, ky, cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
        kf = cv2.remap(vpos, kx, ky, cv2.INTER_NEAREST, borderMode=cv2.BORDER_CONSTANT)
        kdepth = (kf[..., 0] - 0.5) * math.sin(t) + (kf[..., 1] - 0.5) * math.cos(t)
        gap = np.where(kf[..., 3] > 0.5, depth - kdepth, np.inf)
        seen = (gap < args.tol) & uv_valid

        # Facing toward this camera: V_obj = (-sin(t), -cos(t), 0)
        facing = np.clip(-(N[..., 0] * math.sin(t) + N[..., 1] * math.cos(t)), 0.0, 1.0)
        # For side views, suppress weight on front-facing head/face so front.png owns the eyes & mouth exclusively
        w_view = priority * (facing ** args.power)
        if yaw in (90, -90):
            front_face = (N[..., 1] < -0.35) & (Z > 0.02)
            w_view = np.where(front_face, w_view * 0.02, w_view)
        elif yaw == 0:
            # Boost front.png on front-facing face region
            front_face = (N[..., 1] < -0.25) & (Z > 0.02)
            w_view = np.where(front_face, w_view * 4.0, w_view)

        w = seen * w_view
        acc += col * w[..., None]
        wsum += w

        # Fallback: best facing visible (or nearest) view
        fscore = np.where(seen, facing + 1.0, facing - np.clip(gap, 0, 2.0))
        better = (fscore > fbest) & uv_valid
        fallback[better] = col[better]
        fbest = np.where(better, fscore, fbest)

    # 5. Resolve final albedo and fill seams/occluded crevices smoothly
    covered = uv_valid & (wsum > 1e-4)
    albedo = np.where(covered[..., None], acc / np.maximum(wsum, 1e-4)[..., None], fallback)
    # Fill tiny uncovered texels & UV chart borders via edge_fill
    albedo_u8 = np.clip(albedo, 0, 255).astype(np.uint8)
    # Smoothly inpaint any uncovered interior holes first, then bleed UV borders outward
    albedo_u8 = edge_fill(albedo_u8, covered, iterations=64)
    # Note: Blender UV v=0 is bottom of image, whereas our ortho UV render has v=1 at top (matching PIL image coords!)
    albedo_img = Image.fromarray(albedo_u8)
    albedo_path = out / "albedo.png"
    albedo_img.save(albedo_path)

    # 6. Export textured GLB
    mat = trimesh.visual.material.PBRMaterial(baseColorTexture=albedo_img, roughnessFactor=0.8, metallicFactor=0.0)
    tex_mesh = trimesh.Trimesh(
        vertices=uv_mesh.vertices,
        faces=uv_mesh.faces,
        process=False,
        visual=trimesh.visual.TextureVisuals(uv=uvs, material=mat),
    )
    tex_glb = out / "textured.glb"
    tex_mesh.export(tex_glb)
    print(json.dumps({"textured_glb": str(tex_glb), "albedo": str(albedo_path), "ious": ious,
                      "covered_pct": round(100.0 * covered.sum() / max(1, uv_valid.sum()), 2)}))


if __name__ == "__main__":
    main()
