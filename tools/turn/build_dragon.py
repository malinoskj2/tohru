"""Blender: build and render a sculpted, glossy pearlescent Sheet-B chibi 3D dragon for B1 and B2.

blender -b -P build_dragon.py -- OUT MODE [options]
MODE:
  toon      - direct glossy pearlescent Sheet-B toon render (B1)
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
ap.add_argument("--turn", type=int, default=30)
ap.add_argument("--hold-end", type=int, default=10)
ap.add_argument("--ease-blend", type=float, default=0.75)
ap.add_argument("--dip", type=float, default=2.5)
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
        if r >= 0.010:
            parts.append(make_ellipsoid(f"{name}_cap{idx}", (x, y, z), (r, r, r), segs=18, rings=12))
    if len(parts) > 1:
        bpy.ops.object.select_all(action="DESELECT")
        for p in parts:
            p.select_set(True)
        bpy.context.view_layer.objects.active = parts[0]
        bpy.ops.object.join()
        ob = bpy.context.active_object
    return ob


def remesh_smooth(ob, voxel=0.007, smooth_iters=10, factor=0.55):
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
    """Symmetric arched, double-scalloped chibi bat-dragon wing sitting cleanly above the curled tail."""
    parts = []
    pts_top = [
        (side * 0.118, 0.056, -0.014, 0.023),
        (side * 0.212, 0.088,  0.014, 0.024),
        (side * 0.292, 0.114, -0.032, 0.020),
        (side * 0.324, 0.132, -0.104, 0.016),
    ]
    parts.append(make_tube(f"{name}_arch", pts_top, res=24, bevel_res=10))
    pts_mid = [
        (side * 0.132, 0.064, -0.034, 0.020),
        (side * 0.216, 0.096, -0.056, 0.022),
        (side * 0.240, 0.114, -0.108, 0.015),
    ]
    parts.append(make_tube(f"{name}_mid", pts_mid, res=16, bevel_res=8))
    parts.append(make_ellipsoid(f"{name}_web1", (side * 0.214, 0.092, -0.044), (0.076, 0.022, 0.052), (10, -side * 16, side * 18)))
    parts.append(make_ellipsoid(f"{name}_web2", (side * 0.278, 0.118, -0.074), (0.052, 0.018, 0.048), (12, -side * 24, side * 20)))

    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    w = bpy.context.active_object
    w.name = name
    return remesh_smooth(w, voxel=0.0065, smooth_iters=8, factor=0.5)


def build_body():
    """Smooth, plush Sheet-B silhouette: round head flowing organically into cinched neck & plump pear belly + 3 axolotl cheek frills."""
    parts = []
    # 1. Plush round chibi head with soft gentle snout
    parts.append(make_ellipsoid("head", (0.0, 0.004, 0.184), (0.196, 0.194, 0.168)))
    parts.append(make_ellipsoid("cheeks", (0.0, -0.018, 0.136), (0.196, 0.188, 0.116)))
    parts.append(make_ellipsoid("snout", (0.0, -0.164, 0.144), (0.116, 0.068, 0.066)))

    # 2. Smooth organic neck transition (no hard snowman ledge!) into plump round pear belly
    parts.append(make_ellipsoid("neck_upper", (0.0, 0.006, 0.048), (0.146, 0.144, 0.100)))
    parts.append(make_ellipsoid("neck", (0.0, 0.006, -0.012), (0.144, 0.142, 0.104)))
    parts.append(make_ellipsoid("chest", (0.0, 0.004, -0.105), (0.184, 0.180, 0.142)))
    parts.append(make_ellipsoid("belly_pear", (0.0, 0.008, -0.258), (0.244, 0.238, 0.198)))
    parts.append(make_ellipsoid("rump", (0.0, 0.015, -0.354), (0.208, 0.202, 0.128)))

    # 3. Sculpted hip/thigh blends connecting hind feet seamlessly into belly
    parts.append(make_ellipsoid("hip_l", (-0.118, -0.112, -0.305), (0.086, 0.090, 0.096), (10, 14, -18)))
    parts.append(make_ellipsoid("hip_r", ( 0.088, -0.112, -0.312), (0.086, 0.090, 0.096), (10, -14, 18)))

    # 4. Three soft axolotl-style curved cheek frill petals per side (smoothly nested so silhouette emergence has zero pop!)
    for side in (-1, 1):
        parts.append(make_ellipsoid(
            f"frill_1_{side}",
            (side * 0.172, 0.068, 0.198),
            (0.062, 0.066, 0.036),
            (10, -side * 18, -side * 16),
        ))
        parts.append(make_ellipsoid(
            f"frill_2_{side}",
            (side * 0.174, 0.078, 0.146),
            (0.060, 0.064, 0.035),
            (6, -side * 10, -side * 14),
        ))
        parts.append(make_ellipsoid(
            f"frill_3_{side}",
            (side * 0.160, 0.078, 0.098),
            (0.052, 0.056, 0.032),
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
    """Sleek plump S-wave golden-peach flame crest + graceful curved lavender ear-horns (no rabbit ears)."""
    # 1. Iconic Sheet B plump S-wave flame crest: bold graceful S-sweep up-right (+X) & back (+Y)
    crest_pts = [
        (-0.084, -0.025, 0.288, 0.074),
        (-0.040,  0.012, 0.348, 0.074),
        ( 0.036,  0.056, 0.388, 0.058),
        ( 0.112,  0.098, 0.412, 0.038),
        ( 0.148,  0.122, 0.456, 0.018),
    ]
    crest = make_tube("crest", crest_pts, res=36, bevel_res=14)
    crest = remesh_smooth(crest, voxel=0.0065, smooth_iters=8, factor=0.5)

    # 2. Right plush curved lavender ear-horn (+X — nestled smoothly beside crest base & curving out-and-up like Sheet B!)
    r_horn_pts = [
        (0.092, 0.018, 0.236, 0.054),
        (0.162, 0.036, 0.252, 0.048),
        (0.222, 0.054, 0.272, 0.038),
        (0.264, 0.066, 0.292, 0.024),
    ]
    r_horn = make_tube("r_horn", r_horn_pts, res=28, bevel_res=12)
    r_horn = remesh_smooth(r_horn, voxel=0.0065, smooth_iters=6, factor=0.5)

    # 3. Swept-back plush curved lavender ear-horn (-X, +Y — curves gracefully back-and-up in profile, tucked behind skull in front)
    l_horn_pts = [
        (-0.068, 0.072, 0.228, 0.052),
        (-0.098, 0.140, 0.250, 0.046),
        (-0.114, 0.202, 0.278, 0.036),
        (-0.120, 0.238, 0.304, 0.024),
    ]
    l_horn = make_tube("l_horn", l_horn_pts, res=28, bevel_res=12)
    l_horn = remesh_smooth(l_horn, voxel=0.0065, smooth_iters=6, factor=0.5)

    return crest, r_horn, l_horn


def build_tail():
    """Plump, chubby Sheet-B coiled snail-shell spiral tail with crisp glossy swirl and zero holes."""
    s = 0.94
    pts = [
        (0.000 * s, 0.000, -0.028 * s, 0.096),
        (0.112 * s, 0.000, -0.066 * s, 0.091),
        (0.226 * s, 0.000, -0.042 * s, 0.084),
        (0.306 * s, 0.000,  0.036 * s, 0.078),
        (0.318 * s, 0.000,  0.134 * s, 0.072),
        (0.264 * s, 0.000,  0.208 * s, 0.065),
        (0.180 * s, 0.000,  0.210 * s, 0.059),
        (0.134 * s, 0.000,  0.146 * s, 0.055),
        (0.168 * s, 0.000,  0.102 * s, 0.052),
        (0.208 * s, 0.000,  0.128 * s, 0.048),
    ]
    tail = make_tube("tail", pts, res=48, bevel_res=18, fill_caps=True)
    bpy.ops.object.shade_smooth()
    return tail


def build_paws_and_feet():
    paws = []
    # Plump pillowy forepaws tucked warmly together on upper chest
    paws.append(make_ellipsoid("paw_l", (-0.064, -0.172, -0.052), (0.070, 0.054, 0.045), (12, -16, -16)))
    paws.append(make_ellipsoid("paw_r", ( 0.064, -0.172, -0.052), (0.070, 0.054, 0.045), (12,  16,  16)))
    # Plump glossy jelly-bean hind feet tucked snugly into sculpted thigh pads
    paws.append(make_ellipsoid("foot_l", (-0.118, -0.192, -0.324), (0.066, 0.060, 0.084), (16,  20, -22)))
    paws.append(make_ellipsoid("foot_r", ( 0.082, -0.196, -0.330), (0.066, 0.060, 0.084), (16, -20,  22)))
    for p in paws:
        bpy.ops.object.shade_smooth()
    return paws


def build_face():
    """Glossy deep-plum eyes with crisp bright catchlights + cute clearly-visible curved warm-plum smile."""
    eyes = []
    catchlights = []
    for side, nm in ((-1, "eye_l"), (1, "eye_r")):
        ey = make_ellipsoid(nm, (side * 0.088, -0.180, 0.174), (0.029, 0.024, 0.036), (0, 0, -side * 24), segs=24, rings=16)
        bpy.ops.object.shade_smooth()
        eyes.append(ey)
        cl = make_ellipsoid(nm + "_hl", (side * 0.088 - 0.008, -0.203, 0.189), (0.0078, 0.0060, 0.0078), segs=14, rings=10)
        bpy.ops.object.shade_smooth()
        catchlights.append(cl)

    smile_pts = [
        (-0.032, -0.215, 0.136, 0.0052),
        (-0.015, -0.223, 0.128, 0.0056),
        ( 0.000, -0.226, 0.126, 0.0058),
        ( 0.015, -0.223, 0.128, 0.0056),
        ( 0.032, -0.215, 0.136, 0.0052),
    ]
    smile = make_tube("smile", smile_pts, res=16, bevel_res=6)
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


def add_pearl_finish(nt, base_col, em, rim_amt=0.36, sheen_amt=0.78, sheen_min=0.92, sheen_max=0.97,
                     broad_amt=0.18, crease_dark=0.38, edge_ink=0.58):
    """Crisp jewel-like glossy pearlescent jelly finish matching Sheet B without washing out color."""
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
    facing = m("SUBTRACT", 1.0, lw.outputs["Facing"])  # 1 at center, 0 at silhouette

    # 1. Juicy translucent cool-lilac directional shading on lower-right (light from upper-left-front)
    kdot = nt.nodes.new("ShaderNodeVectorMath"); kdot.operation = "DOT_PRODUCT"
    nt.links.new(vt.outputs[0], kdot.inputs[0]); kdot.inputs[1].default_value = (-0.34, 0.50, -0.80)
    sh_fac = m("MULTIPLY", smooth_step(nt, kdot.outputs["Value"], 0.42, -0.45), crease_dark)
    col = mix_rgba(nt, sh_fac, base_col, srgb_to_lin(126, 98, 184))

    # 2. Luminous translucent pearl Fresnel rim (peaks just inside the contour)
    rim_w = m("MULTIPLY", smooth_step(nt, facing, 0.48, 0.15), rim_amt)
    col = mix_rgba(nt, rim_w, col, srgb_to_lin(252, 246, 255))

    # 3. Crisp Sheet-B glossy jelly specular ovals (upper-left-front: (-0.28, 0.56, -0.78))
    sdot = nt.nodes.new("ShaderNodeVectorMath"); sdot.operation = "DOT_PRODUCT"
    nt.links.new(vt.outputs[0], sdot.inputs[0]); sdot.inputs[1].default_value = (-0.28, 0.56, -0.78)
    broad_w = m("MULTIPLY", smooth_step(nt, sdot.outputs["Value"], 0.80, 0.93), broad_amt)
    col = mix_rgba(nt, broad_w, col, (1.0, 0.99, 1.0, 1.0))
    sharp_w = m("MULTIPLY", smooth_step(nt, sdot.outputs["Value"], sheen_min, sheen_max), sheen_amt)
    col = mix_rgba(nt, sharp_w, col, (1.0, 1.0, 1.0, 1.0))

    # 4. Crisp soft-lilac grazing contour (cleanly separates paws, feet, wings, crest, horns & coiled tail spiral)
    if edge_ink > 0:
        ink_w = m("MULTIPLY", smooth_step(nt, facing, 0.25, 0.04), edge_ink)
        col = mix_rgba(nt, ink_w, col, srgb_to_lin(142, 108, 184))

    nt.links.new(col, em.inputs[0])


def make_body_material(pivot_obj):
    """Luminous iridescent Sheet-B pastel rainbow gradient + subtle rosy cheek blush (balanced mint/blue/lavender)."""
    C_PEACH = srgb_to_lin(252, 182, 146)
    C_BLUSH = srgb_to_lin(244, 138, 148)
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

        # Balanced Sheet-B iridescent belly: soft mint-aqua center fading generously into sky-blue & lavender flanks
        diag = math_op("ADD", math_op("MULTIPLY", X, 0.36), math_op("MULTIPLY", Y, 0.65))
        mint_to_blue = smooth_step(nt, diag, -0.22, -0.02)
        belly_base = mix_rgba(nt, mint_to_blue, C_MINT, C_BLUE)
        blue_to_lav = smooth_step(nt, math_op("ADD", diag, math_op("MULTIPLY", Z, -0.22)), -0.06, 0.22)
        lower_col = mix_rgba(nt, blue_to_lav, belly_base, C_LAV)

        # Soft pearly chest/neck bridge
        neck_band = math_op("MULTIPLY", smooth_step(nt, Z, -0.10, 0.02), smooth_step(nt, Z, 0.14, 0.02))
        lower_col = mix_rgba(nt, math_op("MULTIPLY", neck_band, 0.52), lower_col, C_NECK)

        # Head: warm apricot-peach forehead fading softly to iridescent lilac toward back & cheek frills
        head_diag = math_op("ADD", math_op("MULTIPLY", Y, 0.85), math_op("MULTIPLY", math_op("ABSOLUTE", X, 0.0), 0.38))
        head_lav = smooth_step(nt, head_diag, 0.00, 0.20)
        head_col = mix_rgba(nt, head_lav, C_PEACH, C_LAV)

        # Rosy cheek blush spots just under outer eye corners (|X|~0.12, Y~-0.16, Z~0.14)
        dx = math_op("SUBTRACT", math_op("ABSOLUTE", X, 0.0), 0.118)
        dy = math_op("SUBTRACT", Y, -0.158)
        dz = math_op("SUBTRACT", Z, 0.142)
        d2 = math_op("ADD", math_op("ADD", math_op("MULTIPLY", dx, dx), math_op("MULTIPLY", dy, dy)), math_op("MULTIPLY", dz, dz))
        blush_w = math_op("MULTIPLY", smooth_step(nt, d2, 0.0045, 0.0004), 0.38)
        head_col = mix_rgba(nt, blush_w, head_col, C_BLUSH)

        h_fac = smooth_step(nt, Z, -0.02, 0.15)
        col = mix_rgba(nt, h_fac, lower_col, head_col)
        add_pearl_finish(nt, col, em, rim_amt=0.34, sheen_amt=0.78, sheen_min=0.945, sheen_max=0.98,
                         broad_amt=0.16, crease_dark=0.38, edge_ink=0.48)

    return make_emission_mat("body_pearl", build)


def make_crest_material(pivot_obj):
    # Richer golden-amber peach + crisp lilac base contour so crest stays unmistakably distinct from forehead & lavender horns
    C_BASE = srgb_to_lin(255, 198, 108)
    C_TIP  = srgb_to_lin(246, 154, 82)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], 0.29, 0.46), C_BASE, C_TIP)
        add_pearl_finish(nt, col, em, rim_amt=0.36, sheen_amt=0.84, sheen_min=0.90, sheen_max=0.96,
                         broad_amt=0.20, crease_dark=0.28, edge_ink=0.66)
    return make_emission_mat("crest_pearl", build)


def make_horn_material(pivot_obj):
    C_BASE = srgb_to_lin(242, 188, 206)
    C_TIP  = srgb_to_lin(196, 154, 236)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], 0.22, 0.36), C_BASE, C_TIP)
        add_pearl_finish(nt, col, em, rim_amt=0.40, sheen_amt=0.80, sheen_min=0.88, sheen_max=0.95,
                         broad_amt=0.20, crease_dark=0.30, edge_ink=0.56)
    return make_emission_mat("horn_pearl", build)


def make_tail_material(pivot_obj):
    C_LAV  = srgb_to_lin(202, 148, 240)
    C_BLUE = srgb_to_lin(150, 196, 242)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], -0.44, -0.24), C_BLUE, C_LAV)
        add_pearl_finish(nt, col, em, rim_amt=0.42, sheen_amt=0.84, sheen_min=0.87, sheen_max=0.94,
                         broad_amt=0.22, crease_dark=0.38, edge_ink=0.62)
    return make_emission_mat("tail_pearl", build)


def make_wing_material(pivot_obj):
    C_LAV  = srgb_to_lin(202, 162, 240)
    C_CYAN = srgb_to_lin(162, 224, 236)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], -0.11, 0.02), C_CYAN, C_LAV)
        add_pearl_finish(nt, col, em, rim_amt=0.40, sheen_amt=0.76, sheen_min=0.87, sheen_max=0.94,
                         broad_amt=0.18, crease_dark=0.32, edge_ink=0.60)
    return make_emission_mat("wing_pearl", build)


def make_paw_material(tint_peach=False):
    col_val = srgb_to_lin(242, 200, 206) if tint_peach else srgb_to_lin(212, 184, 240)
    def build(nt, em):
        rgb = nt.nodes.new("ShaderNodeRGB"); rgb.outputs[0].default_value = col_val
        add_pearl_finish(nt, rgb.outputs[0], em, rim_amt=0.40, sheen_amt=0.86, sheen_min=0.87, sheen_max=0.94,
                         broad_amt=0.20, crease_dark=0.34, edge_ink=0.62)
    return make_emission_mat("paw_pearl" + ("_p" if tint_peach else ""), build)


def make_eye_material():
    C_DARK = srgb_to_lin(74, 28, 66)
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


# --- ASSEMBLE RIG ---
pivot = bpy.data.objects.new("pivot", None)
link_obj(pivot)

body = build_body()
crest, r_horn, l_horn = build_crest_and_horns()
wing_l = make_wing("wing_l", side=-1)
wing_r = make_wing("wing_r", side=1)
paws = build_paws_and_feet()
eyes, catchlights, smile = build_face()

tail_pivot = bpy.data.objects.new("tail_pivot", None)
tail_pivot.location = (0.0, 0.042, -0.355)
link_obj(tail_pivot)
tail_pivot.parent = pivot
tail = build_tail()
tail.parent = tail_pivot

all_parts = [body, crest, r_horn, l_horn, wing_l, wing_r, tail, smile] + paws + eyes + catchlights
for ob in [body, crest, r_horn, l_horn, wing_l, wing_r, smile] + paws + eyes + catchlights:
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


def tail_angle(yaw):
    # Smooth articulation matching Sheet B profile-left -> front-right with minimal silhouette dip
    frac = max(0.0, min(1.0, yaw / 90.0))
    return 78.0 * (frac ** 0.72)


def pose(yaw, dip):
    pivot.rotation_euler = (math.radians(dip), 0, math.radians(yaw))
    tail_pivot.rotation_euler = (0, 0, math.radians(tail_angle(yaw)))


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
    tail_degs = [tail_angle(y) for y in yaws]
    with open(f"{args.out}/../yaws.json", "w") as fh:
        json.dump(yaws, fh)
    with open(f"{args.out}/../poses.json", "w") as fh:
        json.dump({
            "yaw": yaws,
            "dip": dips,
            "tail_deg": tail_degs,
            "tail_pivot": list(tail_pivot.location),
            "ortho": cam_data.ortho_scale,
            "size": args.size,
        }, fh)
    for f in range(total):
        pose(yaws[f], dips[f])
        scene.render.filepath = f"{args.out}/{f:04d}.png"
        bpy.ops.render.render(write_still=True)
