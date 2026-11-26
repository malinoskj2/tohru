"""Blender: build and render a Sheet-B-accurate illustrated glossy chibi 3D dragon for B1 and B2.

blender -b -P build_dragon.py -- OUT MODE [options]
MODE:
  toon      - direct illustrated pearlescent Sheet-B toon render (B1)
  position  - 16-bit object-space XYZ guide (B2)
  normal    - 16-bit camera-space normal guide (B2)
  alpha     - silhouette alpha mask
  preview   - render 5-angle comparison stills at 90, 67.5, 45, 22.5, 0
"""
import argparse
import json
import math
import os
import sys

import bpy
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
ap = argparse.ArgumentParser()
ap.add_argument("out")
ap.add_argument("mode", choices=("toon", "position", "normal", "alpha", "preview"))
ap.add_argument("--yaw0", type=float, default=90.0)
ap.add_argument("--yaw1", type=float, default=0.0)
ap.add_argument("--hold", type=int, default=4)
ap.add_argument("--turn", type=int, default=36)
ap.add_argument("--hold-end", type=int, default=10)
ap.add_argument("--ease-blend", type=float, default=0.60)
ap.add_argument("--dip", type=float, default=2.2)
ap.add_argument("--size", type=int, default=480)
ap.add_argument("--depth16", action="store_true")
ap.add_argument("--at", type=float, default=None)
args = ap.parse_args(argv)

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene


def link_obj(obj):
    scene.collection.objects.link(obj)
    return obj


def sock(sockets, name):
    for s in sockets:
        if s.name == name and s.type == "RGBA" and s.enabled:
            return s
    return sockets[name]


def make_ellipsoid(name, loc, scale, rot=(0, 0, 0), segs=36, rings=24):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=segs, ring_count=rings, radius=1.0, location=loc)
    ob = bpy.context.active_object
    ob.name = name
    ob.scale = scale
    ob.rotation_euler = tuple(math.radians(a) for a in rot)
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    return ob


def make_tube(name, points_radii, res=32, bevel_res=14, fill_caps=True):
    cu = bpy.data.curves.new(name + "_cu", "CURVE")
    cu.dimensions = "3D"
    cu.resolution_u = res
    cu.bevel_depth = 1.0
    cu.bevel_resolution = bevel_res
    cu.use_fill_caps = fill_caps
    sp = cu.splines.new("NURBS")
    sp.points.add(len(points_radii) - 1)
    for i, (x, y, z, r) in enumerate(points_radii):
        sp.points[i].co = (x, y, z, 1.0)
        sp.points[i].radius = r
    sp.use_endpoint_u = True
    sp.order_u = min(4, len(points_radii))
    ob = bpy.data.objects.new(name, cu)
    link_obj(ob)
    bpy.context.view_layer.objects.active = ob
    bpy.ops.object.select_all(action="DESELECT")
    ob.select_set(True)
    bpy.ops.object.convert(target="MESH")
    ob = bpy.context.active_object
    parts = [ob]
    for idx, (x, y, z, r) in ((0, points_radii[0]), (-1, points_radii[-1])):
        if r >= 0.008:
            parts.append(make_ellipsoid(f"{name}_cap{idx}", (x, y, z), (r, r, r), segs=18, rings=12))
    if len(parts) > 1:
        bpy.ops.object.select_all(action="DESELECT")
        for p in parts:
            p.select_set(True)
        bpy.context.view_layer.objects.active = parts[0]
        bpy.ops.object.join()
        ob = bpy.context.active_object
    return ob


def remesh_smooth(ob, voxel=0.0065, smooth_iters=10, factor=0.55):
    bpy.ops.object.select_all(action="DESELECT")
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob
    rem = ob.modifiers.new("remesh", "REMESH")
    rem.mode = "VOXEL"
    rem.voxel_size = voxel
    rem.use_smooth_shade = True
    bpy.ops.object.modifier_apply(modifier="remesh")
    sm = ob.modifiers.new("smooth", "SMOOTH")
    sm.factor = factor
    sm.iterations = smooth_iters
    bpy.ops.object.modifier_apply(modifier="smooth")
    bpy.ops.object.shade_smooth()
    return ob


def make_wing(name, side=1):
    """True arched double-scalloped Sheet-B bat wing angled back-and-out so near wing is clearly visible from frame 0!"""
    # Build in local wing plane (+U outward/back, +Z up), then rotate ~38 deg back around Z so it reads in BOTH profile & front!
    parts = []
    # High curved upper bat-wing arm/rib curving up then sweeping down to outer tip
    rib_top = [
        (0.000, 0.000,  0.000, 0.022),
        (0.072, 0.000,  0.044, 0.023),
        (0.155, 0.000,  0.012, 0.020),
        (0.212, 0.000, -0.078, 0.014),
    ]
    parts.append(make_tube(f"{name}_rib_top", rib_top, res=24, bevel_res=10))
    # Middle finger rib forming the classic two-scallop bat wing silhouette
    rib_mid = [
        (0.045, 0.000,  0.015, 0.018),
        (0.105, 0.000, -0.020, 0.018),
        (0.130, 0.000, -0.074, 0.013),
    ]
    parts.append(make_tube(f"{name}_rib_mid", rib_mid, res=16, bevel_res=8))
    # Inner finger rib
    rib_in = [
        (0.012, 0.000, -0.008, 0.018),
        (0.048, 0.000, -0.048, 0.015),
    ]
    parts.append(make_tube(f"{name}_rib_in", rib_in, res=12, bevel_res=8))
    # Two curved scalloped bat-wing membrane arches (concave cutouts along bottom!)
    parts.append(make_ellipsoid(f"{name}_web1", (0.078, 0.000, -0.004), (0.046, 0.015, 0.036), (0, -14, 0)))
    parts.append(make_ellipsoid(f"{name}_web2", (0.148, 0.000, -0.022), (0.044, 0.014, 0.038), (0, -32, 0)))

    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    w = bpy.context.active_object
    w.name = name
    w = remesh_smooth(w, voxel=0.006, smooth_iters=7, factor=0.5)
    # Mirror for left side and place on upper-back flank, angled 30 deg back so BOTH profile & front show a real scalloped bat wing!
    if side < 0:
        w.scale = (-1.10, 1.0, 1.10)
        bpy.ops.object.transform_apply(scale=True)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.mesh.normals_make_consistent(inside=False)
        bpy.ops.object.mode_set(mode="OBJECT")
    else:
        w.scale = (1.10, 1.0, 1.10)
        bpy.ops.object.transform_apply(scale=True)
    w.rotation_euler = (math.radians(6), math.radians(-side * 6), math.radians(side * 30))
    w.location = (side * 0.128, 0.102, -0.022)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.object.shade_smooth()
    return w


def build_body():
    """True Sheet-B proportions: BIG plush chibi head nestled directly on a short squat pear belly (NO snowman neck!)."""
    parts = []
    # 1. BIG plush round chibi head (matches Sheet B's big-head-on-squat-body ratio!)
    parts.append(make_ellipsoid("head", (0.0, 0.002, 0.148), (0.218, 0.212, 0.184)))
    parts.append(make_ellipsoid("cheeks", (0.0, -0.022, 0.100), (0.216, 0.204, 0.126)))
    parts.append(make_ellipsoid("snout", (0.0, -0.184, 0.108), (0.124, 0.070, 0.070)))

    # 2. Short tucked neck -> plump squat teardrop belly (forepaws sit right below chin, exact Sheet B!)
    parts.append(make_ellipsoid("neck", (0.0, 0.006, -0.012), (0.156, 0.152, 0.082)))
    parts.append(make_ellipsoid("chest", (0.0, 0.004, -0.092), (0.194, 0.190, 0.128)))
    parts.append(make_ellipsoid("belly_pear", (0.0, 0.008, -0.228), (0.238, 0.232, 0.182)))
    parts.append(make_ellipsoid("rump", (0.0, 0.016, -0.312), (0.202, 0.198, 0.122)))

    # 3. Sculpted hip/thigh blends so hind feet sit naturally in belly
    parts.append(make_ellipsoid("hip_l", (-0.114, -0.108, -0.275), (0.084, 0.088, 0.092), (10, 14, -18)))
    parts.append(make_ellipsoid("hip_r", ( 0.086, -0.108, -0.282), (0.084, 0.088, 0.092), (10, -14, 18)))

    # 4. Three lush swept axolotl cheek frills per side (softly layered back-and-out, exact Sheet B!)
    for side in (-1, 1):
        parts.append(make_ellipsoid(
            f"frill_1_{side}",
            (side * 0.194, 0.072, 0.168),
            (0.068, 0.074, 0.036),
            (10, -side * 20, -side * 16),
        ))
        parts.append(make_ellipsoid(
            f"frill_2_{side}",
            (side * 0.196, 0.088, 0.114),
            (0.066, 0.072, 0.035),
            (6, -side * 12, -side * 14),
        ))
        parts.append(make_ellipsoid(
            f"frill_3_{side}",
            (side * 0.178, 0.088, 0.064),
            (0.056, 0.062, 0.032),
            (2, -side * 4, -side * 12),
        ))

    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    body = bpy.context.active_object
    body.name = "body"
    return remesh_smooth(body, voxel=0.007, smooth_iters=14, factor=0.55)


def build_crest_and_horns():
    """Bold swept S-wave golden-peach flame crest (reads as a flame wave in BOTH profile AND front, never a cat ear!)
    + lush curved lavender ear-horns."""
    # 1. Swept S-wave flame crest: curves generously BACK (+Y) AND UP-RIGHT (+X) so profile shows the FULL S-wave flame!
    crest_pts = [
        (-0.074, -0.075, 0.258, 0.076),
        (-0.034, -0.002, 0.324, 0.076),
        ( 0.032,  0.095, 0.362, 0.058),
        ( 0.102,  0.178, 0.386, 0.038),
        ( 0.136,  0.228, 0.436, 0.018),
    ]
    crest = make_tube("crest", crest_pts, res=36, bevel_res=14)
    crest = remesh_smooth(crest, voxel=0.0065, smooth_iters=8, factor=0.5)

    # 2. Right plush curved lavender ear-horn (+X — nestled smoothly beside crest base & curving out-and-up like Sheet B!)
    r_horn_pts = [
        (0.104, 0.024, 0.214, 0.056),
        (0.178, 0.048, 0.232, 0.050),
        (0.242, 0.074, 0.254, 0.040),
        (0.284, 0.092, 0.278, 0.026),
    ]
    r_horn = make_tube("r_horn", r_horn_pts, res=28, bevel_res=12)
    r_horn = remesh_smooth(r_horn, voxel=0.0065, smooth_iters=6, factor=0.5)

    # 3. Swept-back plush curved lavender ear-horn (-X, +Y — lush curved horn matching Sheet B profile, NOT a thin stick!)
    l_horn_pts = [
        (-0.082, 0.088, 0.204, 0.056),
        (-0.118, 0.162, 0.228, 0.050),
        (-0.136, 0.230, 0.262, 0.038),
        (-0.138, 0.272, 0.296, 0.024),
    ]
    l_horn = make_tube("l_horn", l_horn_pts, res=28, bevel_res=12)
    l_horn = remesh_smooth(l_horn, voxel=0.0065, smooth_iters=6, factor=0.5)

    return crest, r_horn, l_horn


def build_tail():
    """100% RIGID 3D plump curled snail-shell spiral tail angled +38 deg back-right!
    Sweeps naturally & solidly in genuine 3D from viewer-left (profile) beneath/behind hip to viewer-right (front), zero holes."""
    s = 0.94
    pts = [
        (0.000 * s, 0.000, -0.025 * s, 0.094),
        (0.122 * s, 0.000, -0.070 * s, 0.088),
        (0.246 * s, 0.000, -0.050 * s, 0.082),
        (0.332 * s, 0.000,  0.028 * s, 0.075),
        (0.344 * s, 0.000,  0.128 * s, 0.068),
        (0.286 * s, 0.000,  0.206 * s, 0.062),
        (0.198 * s, 0.000,  0.206 * s, 0.057),
        (0.154 * s, 0.000,  0.142 * s, 0.054),
        (0.192 * s, 0.000,  0.100 * s, 0.052),
        (0.230 * s, 0.000,  0.128 * s, 0.050),
    ]
    tail = make_tube("tail", pts, res=48, bevel_res=18, fill_caps=True)
    tail.rotation_euler = (0.0, 0.0, math.radians(38.0))
    tail.location = (0.030, 0.100, -0.312)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.object.shade_smooth()
    return tail


def build_paws_and_feet():
    paws = []
    # Plump pillowy forepaws nestled warmly right under the chin (exact Sheet B!)
    paws.append(make_ellipsoid("paw_l", (-0.068, -0.186, -0.048), (0.072, 0.056, 0.046), (12, -16, -16)))
    paws.append(make_ellipsoid("paw_r", ( 0.068, -0.186, -0.048), (0.072, 0.056, 0.046), (12,  16,  16)))
    # Plump glossy jelly-bean hind feet tucked snugly into sculpted thigh pads
    paws.append(make_ellipsoid("foot_l", (-0.114, -0.190, -0.292), (0.066, 0.060, 0.082), (16,  20, -22)))
    paws.append(make_ellipsoid("foot_r", ( 0.084, -0.194, -0.298), (0.066, 0.060, 0.082), (16, -20,  22)))
    for p in paws:
        bpy.ops.object.shade_smooth()
    return paws


def build_face():
    """Larger glossy Sheet-B plum-violet eyes with crisp catchlights + clear curved U-smile (corners visible on snout!)."""
    eyes = []
    catchlights = []
    for side, nm in ((-1, "eye_l"), (1, "eye_r")):
        ey = make_ellipsoid(nm, (side * 0.094, -0.200, 0.144), (0.033, 0.026, 0.041), (0, 0, -side * 24), segs=24, rings=16)
        bpy.ops.object.shade_smooth()
        eyes.append(ey)
        cl = make_ellipsoid(nm + "_hl", (side * 0.094 - 0.009, -0.224, 0.160), (0.0092, 0.0070, 0.0092), segs=14, rings=10)
        bpy.ops.object.shade_smooth()
        catchlights.append(cl)

    # Clear curved U-smile following the rounded snout contour so BOTH upturned corners stay cleanly visible!
    smile_pts = [
        (-0.038, -0.244, 0.110, 0.0056),
        (-0.020, -0.251, 0.096, 0.0060),
        ( 0.000, -0.253, 0.091, 0.0062),
        ( 0.020, -0.251, 0.096, 0.0060),
        ( 0.038, -0.244, 0.110, 0.0056),
    ]
    smile = make_tube("smile", smile_pts, res=20, bevel_res=6)
    bpy.ops.object.shade_smooth()
    return eyes, catchlights, smile


# --- SHADERS ---
def srgb_to_lin(r, g, b):
    return ((r / 255.0) ** 2.2, (g / 255.0) ** 2.2, (b / 255.0) ** 2.2, 1.0)


def make_emission_mat(name, build_fn):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(em.outputs[0], out.inputs[0])
    build_fn(nt, em)
    return mat


def mix_rgba(nt, fac, ca, cb):
    mx = nt.nodes.new("ShaderNodeMix")
    mx.data_type = "RGBA"
    mx.clamp_factor = True
    f_in = mx.inputs["Factor"]
    a_in = sock(mx.inputs, "A")
    b_in = sock(mx.inputs, "B")
    if isinstance(fac, (int, float)): f_in.default_value = fac
    else: nt.links.new(fac, f_in)
    if isinstance(ca, tuple): a_in.default_value = ca
    else: nt.links.new(ca, a_in)
    if isinstance(cb, tuple): b_in.default_value = cb
    else: nt.links.new(cb, b_in)
    return sock(mx.outputs, "Result")


def smooth_step(nt, val, lo, hi):
    if lo <= hi:
        n = nt.nodes.new("ShaderNodeMapRange")
        n.inputs["From Min"].default_value = lo
        n.inputs["From Max"].default_value = hi
        n.interpolation_type = "SMOOTHSTEP"
        nt.links.new(val, n.inputs["Value"])
        return n.outputs["Result"]
    else:
        n = nt.nodes.new("ShaderNodeMapRange")
        n.inputs["From Min"].default_value = hi
        n.inputs["From Max"].default_value = lo
        n.interpolation_type = "SMOOTHSTEP"
        nt.links.new(val, n.inputs["Value"])
        inv = nt.nodes.new("ShaderNodeMath")
        inv.operation = "SUBTRACT"
        inv.inputs[0].default_value = 1.0
        nt.links.new(n.outputs["Result"], inv.inputs[1])
        return inv.outputs[0]


def add_pearl_finish(nt, base_col, em, rim_amt=0.34, sheen_amt=0.80, sheen_min=0.935, sheen_max=0.975,
                     broad_amt=0.16, crease_dark=0.36, edge_ink=0.76):
    """Illustrated Sheet-B glossy pearlescent jelly finish with crisp plum-lavender ink linework."""
    def m(op, a, b):
        n = nt.nodes.new("ShaderNodeMath"); n.operation = op
        for i, x in enumerate((a, b)):
            if isinstance(x, (int, float)): n.inputs[i].default_value = x
            else: nt.links.new(x, n.inputs[i])
        return n.outputs[0]

    geo = nt.nodes.new("ShaderNodeNewGeometry")
    vt = nt.nodes.new("ShaderNodeVectorTransform")
    vt.vector_type, vt.convert_from, vt.convert_to = "NORMAL", "WORLD", "CAMERA"
    nt.links.new(geo.outputs["Normal"], vt.inputs[0])

    lw = nt.nodes.new("ShaderNodeLayerWeight"); lw.inputs["Blend"].default_value = 0.5
    facing = m("SUBTRACT", 1.0, lw.outputs["Facing"])

    # 1. Translucent cool-lilac directional shading on lower-right (camera normal faces -Z)
    kdot = nt.nodes.new("ShaderNodeVectorMath"); kdot.operation = "DOT_PRODUCT"
    nt.links.new(vt.outputs[0], kdot.inputs[0]); kdot.inputs[1].default_value = (-0.34, 0.50, -0.80)
    sh_fac = m("MULTIPLY", smooth_step(nt, kdot.outputs["Value"], 0.42, -0.45), crease_dark)
    col = mix_rgba(nt, sh_fac, base_col, srgb_to_lin(126, 98, 182))

    # 2. Luminous pearl Fresnel rim inside the ink line
    rim_w = m("MULTIPLY", smooth_step(nt, facing, 0.50, 0.22), rim_amt)
    col = mix_rgba(nt, rim_w, col, srgb_to_lin(252, 246, 255))

    # 3. Crisp Sheet-B glossy jelly specular ovals (placed cleanly on upper forehead & upper belly)
    sdot = nt.nodes.new("ShaderNodeVectorMath"); sdot.operation = "DOT_PRODUCT"
    nt.links.new(vt.outputs[0], sdot.inputs[0]); sdot.inputs[1].default_value = (-0.22, 0.64, -0.73)
    broad_w = m("MULTIPLY", smooth_step(nt, sdot.outputs["Value"], 0.80, 0.93), broad_amt)
    col = mix_rgba(nt, broad_w, col, (1.0, 0.99, 1.0, 1.0))
    sharp_w = m("MULTIPLY", smooth_step(nt, sdot.outputs["Value"], sheen_min, sheen_max), sheen_amt)
    col = mix_rgba(nt, sharp_w, col, (1.0, 1.0, 1.0, 1.0))

    # 4. Crisp Sheet-B illustrated plum-lavender ink contour on grazing angles (separates paws, feet, wings, crest, horns & tail coil!)
    if edge_ink > 0:
        ink_w = m("MULTIPLY", smooth_step(nt, facing, 0.30, 0.08), edge_ink)
        col = mix_rgba(nt, ink_w, col, srgb_to_lin(114, 82, 144))

    nt.links.new(col, em.inputs[0])


def make_body_material(pivot_obj):
    """Luminous iridescent Sheet-B pastel rainbow gradient (clean cheeks, NO extra blush!)."""
    C_PEACH = srgb_to_lin(252, 184, 144)
    C_LAV   = srgb_to_lin(192, 154, 232)
    C_MINT  = srgb_to_lin(168, 224, 198)
    C_BLUE  = srgb_to_lin(154, 194, 238)
    C_NECK  = srgb_to_lin(222, 198, 234)

    def build(nt, em):
        def math_op(op, a, b):
            n = nt.nodes.new("ShaderNodeMath"); n.operation = op
            for i, x in enumerate((a, b)):
                if isinstance(x, (int, float)): n.inputs[i].default_value = x
                else: nt.links.new(x, n.inputs[i])
            return n.outputs[0]

        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        X, Y, Z = sep.outputs["X"], sep.outputs["Y"], sep.outputs["Z"]

        # Balanced Sheet-B iridescent belly: soft mint-aqua front fading smoothly into sky-blue & lavender flanks
        diag = math_op("ADD", math_op("MULTIPLY", X, 0.36), math_op("MULTIPLY", Y, 0.65))
        mint_to_blue = smooth_step(nt, diag, -0.22, -0.02)
        belly_base = mix_rgba(nt, mint_to_blue, C_MINT, C_BLUE)
        blue_to_lav = smooth_step(nt, math_op("ADD", diag, math_op("MULTIPLY", Z, -0.22)), -0.06, 0.22)
        lower_col = mix_rgba(nt, blue_to_lav, belly_base, C_LAV)

        # Soft pearly chest bridge
        neck_band = math_op("MULTIPLY", smooth_step(nt, Z, -0.10, -0.01), smooth_step(nt, Z, 0.10, -0.01))
        lower_col = mix_rgba(nt, math_op("MULTIPLY", neck_band, 0.48), lower_col, C_NECK)

        # Head: warm apricot-peach forehead fading softly to iridescent lilac toward back & cheek frills
        head_diag = math_op("ADD", math_op("MULTIPLY", Y, 0.82), math_op("MULTIPLY", math_op("ABSOLUTE", X, 0.0), 0.36))
        head_lav = smooth_step(nt, head_diag, 0.00, 0.22)
        head_col = mix_rgba(nt, head_lav, C_PEACH, C_LAV)

        h_fac = smooth_step(nt, Z, -0.04, 0.12)
        col = mix_rgba(nt, h_fac, lower_col, head_col)
        add_pearl_finish(nt, col, em, rim_amt=0.34, sheen_amt=0.78, sheen_min=0.945, sheen_max=0.98,
                         broad_amt=0.16, crease_dark=0.38, edge_ink=0.68)

    return make_emission_mat("body_pearl", build)


def make_crest_material(pivot_obj):
    C_BASE = srgb_to_lin(255, 198, 112)
    C_TIP  = srgb_to_lin(246, 154, 86)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], 0.26, 0.43), C_BASE, C_TIP)
        add_pearl_finish(nt, col, em, rim_amt=0.36, sheen_amt=0.84, sheen_min=0.90, sheen_max=0.96,
                         broad_amt=0.20, crease_dark=0.28, edge_ink=0.78)
    return make_emission_mat("crest_pearl", build)


def make_horn_material(pivot_obj):
    C_BASE = srgb_to_lin(242, 188, 206)
    C_TIP  = srgb_to_lin(196, 154, 236)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], 0.20, 0.32), C_BASE, C_TIP)
        add_pearl_finish(nt, col, em, rim_amt=0.40, sheen_amt=0.80, sheen_min=0.88, sheen_max=0.95,
                         broad_amt=0.20, crease_dark=0.30, edge_ink=0.76)
    return make_emission_mat("horn_pearl", build)


def make_tail_material(pivot_obj):
    C_LAV  = srgb_to_lin(202, 148, 240)
    C_BLUE = srgb_to_lin(150, 196, 242)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], -0.40, -0.22), C_BLUE, C_LAV)
        add_pearl_finish(nt, col, em, rim_amt=0.42, sheen_amt=0.84, sheen_min=0.87, sheen_max=0.94,
                         broad_amt=0.22, crease_dark=0.38, edge_ink=0.82)
    return make_emission_mat("tail_pearl", build)


def make_wing_material(pivot_obj):
    C_LAV  = srgb_to_lin(202, 162, 240)
    C_CYAN = srgb_to_lin(162, 224, 236)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], -0.12, 0.01), C_CYAN, C_LAV)
        add_pearl_finish(nt, col, em, rim_amt=0.40, sheen_amt=0.76, sheen_min=0.87, sheen_max=0.94,
                         broad_amt=0.18, crease_dark=0.34, edge_ink=0.84)
    return make_emission_mat("wing_pearl", build)


def make_paw_material(tint_peach=False):
    col_val = srgb_to_lin(242, 200, 206) if tint_peach else srgb_to_lin(212, 184, 240)
    def build(nt, em):
        rgb = nt.nodes.new("ShaderNodeRGB"); rgb.outputs[0].default_value = col_val
        add_pearl_finish(nt, rgb.outputs[0], em, rim_amt=0.40, sheen_amt=0.86, sheen_min=0.87, sheen_max=0.94,
                         broad_amt=0.20, crease_dark=0.34, edge_ink=0.82)
    return make_emission_mat("paw_pearl" + ("_p" if tint_peach else ""), build)


def make_eye_material():
    C_DARK = srgb_to_lin(72, 26, 66)
    C_ROSE = srgb_to_lin(168, 92, 134)
    def build(nt, em):
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        vt = nt.nodes.new("ShaderNodeVectorTransform")
        vt.vector_type, vt.convert_from, vt.convert_to = "NORMAL", "WORLD", "CAMERA"
        nt.links.new(geo.outputs["Normal"], vt.inputs[0])
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(vt.outputs[0], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Y"], -0.65, -0.15), C_ROSE, C_DARK)
        nt.links.new(col, em.inputs[0])
    return make_emission_mat("eye_mat", build)


def make_guide_mat(mode, pivot_obj):
    if mode == "position":
        def build(nt, em):
            tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
            mp = nt.nodes.new("ShaderNodeMapping")
            mp.inputs["Location"].default_value = (0.5, 0.5, 0.5)
            nt.links.new(tc.outputs["Object"], mp.inputs[0])
            nt.links.new(mp.outputs[0], em.inputs[0])
        return make_emission_mat("pos", build)
    elif mode == "normal":
        def build(nt, em):
            geo = nt.nodes.new("ShaderNodeNewGeometry")
            vt = nt.nodes.new("ShaderNodeVectorTransform")
            vt.vector_type, vt.convert_from, vt.convert_to = "NORMAL", "WORLD", "CAMERA"
            mp = nt.nodes.new("ShaderNodeMapping")
            mp.inputs["Location"].default_value = (0.5, 0.5, 0.5)
            mp.inputs["Scale"].default_value = (0.5, 0.5, 0.5)
            nt.links.new(geo.outputs["Normal"], vt.inputs[0])
            nt.links.new(vt.outputs[0], mp.inputs[0])
            nt.links.new(mp.outputs[0], em.inputs[0])
        return make_emission_mat("nrm", build)
    else:
        return make_emission_mat("white", lambda nt, em: setattr(em.inputs[0], "default_value", (1, 1, 1, 1)))


# --- ASSEMBLE RIG (100% rigid in pivot space!) ---
pivot = bpy.data.objects.new("pivot", None)
link_obj(pivot)

body = build_body()
crest, r_horn, l_horn = build_crest_and_horns()
wing_l = make_wing("wing_l", side=-1)
wing_r = make_wing("wing_r", side=1)
paws = build_paws_and_feet()
eyes, catchlights, smile = build_face()
tail = build_tail()

all_parts = [body, crest, r_horn, l_horn, wing_l, wing_r, tail, smile] + paws + eyes + catchlights
for ob in all_parts:
    ob.parent = pivot

if args.mode in ("toon", "preview"):
    m_body = make_body_material(pivot)
    m_crest = make_crest_material(pivot)
    m_horn = make_horn_material(pivot)
    m_tail = make_tail_material(pivot)
    m_wing = make_wing_material(pivot)
    m_paw_p = make_paw_material(tint_peach=True)
    m_paw_l = make_paw_material(tint_peach=False)
    m_eye = make_eye_material()
    m_white = make_emission_mat("catchlight", lambda nt, em: setattr(em.inputs[0], "default_value", (1.0, 1.0, 1.0, 1.0)))
    m_smile = make_emission_mat("smile_mat", lambda nt, em: setattr(em.inputs[0], "default_value", srgb_to_lin(76, 28, 72)))

    body.data.materials.append(m_body)
    crest.data.materials.append(m_crest)
    r_horn.data.materials.append(m_horn)
    l_horn.data.materials.append(m_horn)
    tail.data.materials.append(m_tail)
    wing_l.data.materials.append(m_wing)
    wing_r.data.materials.append(m_wing)
    paws[0].data.materials.append(m_paw_p)
    paws[1].data.materials.append(m_paw_l)
    paws[2].data.materials.append(m_paw_p)
    paws[3].data.materials.append(m_paw_l)
    for e in eyes: e.data.materials.append(m_eye)
    for c in catchlights: c.data.materials.append(m_white)
    smile.data.materials.append(m_smile)
else:
    gmat = make_guide_mat(args.mode, pivot)
    for ob in all_parts:
        ob.data.materials.clear()
        ob.data.materials.append(gmat)

scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = scene.render.resolution_y = args.size
scene.render.film_transparent = True
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGBA"
if args.depth16:
    scene.render.image_settings.color_depth = "16"
scene.view_settings.view_transform = "Raw" if args.mode in ("position", "normal") else "Standard"
scene.render.filter_size = 1.0

cam_data = bpy.data.cameras.new("cam")
cam_data.type = "ORTHO"
cam_data.ortho_scale = 1.25
cam = bpy.data.objects.new("cam", cam_data)
link_obj(cam)
cam.location = (0, -5, 0)
cam.rotation_euler = (math.radians(90), 0, 0)
scene.camera = cam
scene.world = bpy.data.worlds.new("w")
scene.world.color = (0, 0, 0)


def ease(u):
    b = args.ease_blend
    return (1.0 - b) * u + b * (u * u * (3 - 2 * u))


def pose(yaw, dip):
    pivot.rotation_euler = (math.radians(dip), 0, math.radians(yaw))


if args.mode == "preview":
    os.makedirs(args.out, exist_ok=True)
    for y in (90, 67.5, 45, 22.5, 0):
        pose(y, 0)
        scene.render.filepath = f"{args.out}/y_{y}.png"
        bpy.ops.render.render(write_still=True)
elif args.at is not None:
    pose(args.at, 0)
    scene.render.filepath = args.out
    bpy.ops.render.render(write_still=True)
else:
    total = args.hold + args.turn + args.hold_end
    os.makedirs(args.out, exist_ok=True)
    yaws = [args.yaw0 + (args.yaw1 - args.yaw0) * ease(min(1.0, max(0.0, (f - args.hold) / max(1, args.turn - 1))))
            for f in range(total)]
    dips = [args.dip * math.sin(math.pi * min(1.0, max(0.0, (f - args.hold) / max(1, args.turn - 1)))) for f in range(total)]
    with open(f"{args.out}/../yaws.json", "w") as fh:
        json.dump(yaws, fh)
    with open(f"{args.out}/../poses.json", "w") as fh:
        json.dump({"yaw": yaws, "dip": dips, "ortho": cam_data.ortho_scale, "size": args.size}, fh)
    for f in range(total):
        pose(yaws[f], dips[f])
        scene.render.filepath = f"{args.out}/{f:04d}.png"
        bpy.ops.render.render(write_still=True)
