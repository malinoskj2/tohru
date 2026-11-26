"""Blender: build and render the Sheet-B-accurate illustrated glossy chibi 3D dragon (B1).

blender -b -P build_dragon.py -- OUT MODE [options]
MODE:
  toon      - direct illustrated pearlescent Sheet-B toon render (B1)
  position  - 16-bit object-space XYZ guide
  normal    - 16-bit camera-space normal guide
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
ap.add_argument("--turn", type=int, default=44)
ap.add_argument("--hold-end", type=int, default=8)
ap.add_argument("--ease-blend", type=float, default=1.0)
ap.add_argument("--dip", type=float, default=2.0)
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
    """Bold, arched double-scalloped Sheet-B lavender bat wing clearly visible in BOTH profile and front."""
    parts = []
    # High curved upper bat-wing arm sweeping boldly out and down to a pointed bat-wing tip
    rib_top = [
        (0.000, 0.000,  0.000, 0.024),
        (0.080, 0.000,  0.048, 0.025),
        (0.164, 0.000,  0.014, 0.021),
        (0.212, 0.000, -0.074, 0.016),
    ]
    parts.append(make_tube(f"{name}_rib_top", rib_top, res=24, bevel_res=10))
    # Middle finger rib forming two crisp concave bat-wing scallops underneath
    rib_mid = [
        (0.052, 0.000,  0.020, 0.019),
        (0.114, 0.000, -0.018, 0.019),
        (0.138, 0.000, -0.074, 0.014),
    ]
    parts.append(make_tube(f"{name}_rib_mid", rib_mid, res=16, bevel_res=8))
    # Inner finger rib
    rib_in = [
        (0.015, 0.000, -0.008, 0.019),
        (0.052, 0.000, -0.052, 0.015),
    ]
    parts.append(make_tube(f"{name}_rib_in", rib_in, res=12, bevel_res=8))
    # Scalloped membrane webbing (filled smoothly into shoulder root so ZERO armpit pinhole ever opens!)
    parts.append(make_ellipsoid(f"{name}_web1", (0.072, 0.000, -0.010), (0.062, 0.016, 0.046), (0, -14, 0)))
    parts.append(make_ellipsoid(f"{name}_web2", (0.156, 0.000, -0.022), (0.050, 0.015, 0.042), (0, -30, 0)))

    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    w = bpy.context.active_object
    w.name = name
    w = remesh_smooth(w, voxel=0.006, smooth_iters=7, factor=0.5)
    if side < 0:
        w.scale = (-1.04, 1.0, 1.04)
        bpy.ops.object.transform_apply(scale=True)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.mesh.normals_make_consistent(inside=False)
        bpy.ops.object.mode_set(mode="OBJECT")
    else:
        w.scale = (1.04, 1.0, 1.04)
        bpy.ops.object.transform_apply(scale=True)
    # Angled 24 deg back & seated snugly into shoulder (side*0.118, z=+0.006) -> BOTH wings show symmetrically above tail, ZERO armpit hole!
    w.rotation_euler = (math.radians(6), math.radians(-side * 6), math.radians(side * 24))
    w.location = (side * 0.118, 0.094, 0.006)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bpy.ops.object.shade_smooth()
    return w


def build_body():
    """True Sheet-B chibi proportions: BIG broad round head (wider than torso!) over a squat smooth round pear belly."""
    parts = []
    # 1. BIG broad round chibi head (wider than belly — exact Sheet B!) with full rounded forehead & upturned snout
    parts.append(make_ellipsoid("head_dome", (0.0, 0.000, 0.162), (0.240, 0.232, 0.192)))
    parts.append(make_ellipsoid("forehead", (0.0, -0.056, 0.190), (0.192, 0.172, 0.136)))
    parts.append(make_ellipsoid("cheeks", (0.0, -0.028, 0.110), (0.242, 0.222, 0.134)))
    parts.append(make_ellipsoid("snout", (0.0, -0.198, 0.112), (0.134, 0.080, 0.074), (10, 0, 0)))

    # 2. ONE single continuous squat round pear belly (compact under the big head, ZERO waist ridge!)
    parts.append(make_ellipsoid("neck_bridge", (0.0, 0.010, 0.000), (0.178, 0.176, 0.110)))
    parts.append(make_ellipsoid("torso_pear", (0.0, 0.008, -0.180), (0.224, 0.220, 0.218)))
    parts.append(make_ellipsoid("rump_base", (0.0, 0.014, -0.286), (0.198, 0.194, 0.128)))

    # 3. Sculpted hip/thigh blends so hind feet sit smoothly in lower belly
    parts.append(make_ellipsoid("hip_l", (-0.106, -0.106, -0.262), (0.080, 0.084, 0.088), (10, 14, -18)))
    parts.append(make_ellipsoid("hip_r", ( 0.084, -0.106, -0.268), (0.080, 0.084, 0.088), (10, -14, 18)))

    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    body = bpy.context.active_object
    body.name = "body"
    return remesh_smooth(body, voxel=0.007, smooth_iters=14, factor=0.55)


def build_frills():
    """Separate sculpted glossy axolotl cheek frills (3 per side) around the broad cheeks with crisp ink & gloss."""
    frills = []
    for side in (-1, 1):
        for idx, (loc, scale, rot) in enumerate((
            ((side * 0.212, 0.072, 0.176), (0.066, 0.074, 0.032), (14, -side * 24, -side * 16)),
            ((side * 0.216, 0.088, 0.122), (0.066, 0.074, 0.032), (8,  -side * 16, -side * 14)),
            ((side * 0.200, 0.088, 0.072), (0.056, 0.064, 0.028), (4,  -side * 8,  -side * 12)),
        )):
            f = make_ellipsoid(f"frill_{idx}_{side}", loc, scale, rot, segs=24, rings=16)
            bpy.ops.object.shade_smooth()
            frills.append(f)
    return frills


def build_crest_and_horns():
    """Iconic Sheet-B swept S-wave golden-peach flame crest + symmetric curved UPWARD lavender dragon horns."""
    crest_pts = [
        (-0.056, -0.106, 0.278, 0.088),
        (-0.020, -0.028, 0.344, 0.084),
        ( 0.038,  0.054, 0.382, 0.066),
        ( 0.098,  0.122, 0.410, 0.044),
        ( 0.128,  0.156, 0.464, 0.022),
    ]
    crest = make_tube("crest", crest_pts, res=36, bevel_res=14)
    crest = remesh_smooth(crest, voxel=0.0065, smooth_iters=8, factor=0.5)

    # Symmetric lush curved UPWARD purple dragon horns clearly distinct above the cheek frills (prominence < 6px -> ZERO SAM crest split!)
    def make_horn(name, side):
        pts = [
            (side * 0.128, 0.040, 0.218, 0.060),
            (side * 0.198, 0.072, 0.236, 0.054),
            (side * 0.252, 0.100, 0.254, 0.042),
            (side * 0.286, 0.118, 0.272, 0.026),
        ]
        h = make_tube(name, pts, res=28, bevel_res=12)
        return remesh_smooth(h, voxel=0.0065, smooth_iters=6, factor=0.5)

    r_horn = make_horn("r_horn", 1)
    l_horn = make_horn("l_horn", -1)
    return crest, r_horn, l_horn


def build_tail():
    """Prominent 100% RIGID 3D Sheet-B curled snail-shell spiral tail positioned cleanly below the right wing!"""
    s = 1.02
    pts = [
        (0.000 * s, 0.000, -0.024 * s, 0.098),
        (0.122 * s, 0.000, -0.070 * s, 0.092),
        (0.248 * s, 0.000, -0.048 * s, 0.086),
        (0.336 * s, 0.000,  0.030 * s, 0.078),
        (0.346 * s, 0.000,  0.132 * s, 0.071),
        (0.286 * s, 0.000,  0.210 * s, 0.064),
        (0.196 * s, 0.000,  0.210 * s, 0.059),
        (0.152 * s, 0.000,  0.144 * s, 0.055),
        (0.190 * s, 0.000,  0.100 * s, 0.052),
        (0.228 * s, 0.000,  0.128 * s, 0.049),
    ]
    tail = make_tube("tail", pts, res=48, bevel_res=18, fill_caps=True)
    bpy.ops.object.shade_smooth()
    tail.rotation_euler = (0.0, 0.0, math.radians(34.0))
    tail.location = (0.048, 0.086, -0.326)
    return tail


def build_paws_and_feet():
    paws = []
    paws.append(make_ellipsoid("paw_l", (-0.064, -0.184, -0.042), (0.068, 0.056, 0.045), (12, -16, -16)))
    paws.append(make_ellipsoid("paw_r", ( 0.064, -0.184, -0.042), (0.068, 0.056, 0.045), (12,  16,  16)))
    paws.append(make_ellipsoid("foot_l", (-0.106, -0.186, -0.278), (0.062, 0.058, 0.078), (16,  20, -22)))
    paws.append(make_ellipsoid("foot_r", ( 0.082, -0.190, -0.284), (0.062, 0.058, 0.078), (16, -20,  22)))
    for p in paws:
        bpy.ops.object.shade_smooth()
    return paws


def build_face():
    """Wide-set larger glossy Sheet-B plum-violet eyes + clear flush curved U-smile on the broad head."""
    eyes = []
    catchlights = []
    for side, nm in ((-1, "eye_l"), (1, "eye_r")):
        ey = make_ellipsoid(nm, (side * 0.112, -0.208, 0.150), (0.035, 0.018, 0.043), (0, 0, -side * 28), segs=24, rings=16)
        bpy.ops.object.shade_smooth()
        eyes.append(ey)
        cl = make_ellipsoid(nm + "_hl", (side * 0.112 - 0.008, -0.224, 0.165), (0.0092, 0.0055, 0.0092), segs=14, rings=10)
        bpy.ops.object.shade_smooth()
        catchlights.append(cl)

    smile_pts = [
        (-0.042, -0.262, 0.116, 0.0054),
        (-0.022, -0.272, 0.102, 0.0056),
        ( 0.000, -0.276, 0.097, 0.0058),
        ( 0.022, -0.272, 0.102, 0.0056),
        ( 0.042, -0.262, 0.116, 0.0054),
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


def math_op(nt, op, a, b=0.0):
    n = nt.nodes.new("ShaderNodeMath"); n.operation = op
    for i, x in enumerate((a, b)):
        if i >= len(n.inputs): break
        if isinstance(x, (int, float)): n.inputs[i].default_value = x
        else: nt.links.new(x, n.inputs[i])
    return n.outputs[0]


def add_pearl_finish(nt, base_col, em, rim_amt=0.34, sheen_amt=0.80, sheen_min=0.935, sheen_max=0.975,
                     broad_amt=0.16, crease_dark=0.36, edge_ink=0.72, ink_rgb=(116, 84, 146)):
    """Illustrated Sheet-B glossy pearlescent jelly finish with customizable contour ink color."""
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    vt = nt.nodes.new("ShaderNodeVectorTransform")
    vt.vector_type, vt.convert_from, vt.convert_to = "NORMAL", "WORLD", "CAMERA"
    nt.links.new(geo.outputs["Normal"], vt.inputs[0])

    lw = nt.nodes.new("ShaderNodeLayerWeight"); lw.inputs["Blend"].default_value = 0.5
    facing = math_op(nt, "SUBTRACT", 1.0, lw.outputs["Facing"])

    # 1. Translucent cool-lilac directional shading on lower-right (camera normal faces -Z)
    kdot = nt.nodes.new("ShaderNodeVectorMath"); kdot.operation = "DOT_PRODUCT"
    nt.links.new(vt.outputs[0], kdot.inputs[0]); kdot.inputs[1].default_value = (-0.34, 0.50, -0.80)
    sh_fac = math_op(nt, "MULTIPLY", smooth_step(nt, kdot.outputs["Value"], 0.42, -0.45), crease_dark)
    col = mix_rgba(nt, sh_fac, base_col, srgb_to_lin(126, 98, 182))

    # 2. Luminous pearl Fresnel rim inside the ink line
    rim_w = math_op(nt, "MULTIPLY", smooth_step(nt, facing, 0.50, 0.22), rim_amt)
    col = mix_rgba(nt, rim_w, col, srgb_to_lin(252, 246, 255))

    # 3. Crisp Sheet-B glossy jelly specular ovals
    sdot = nt.nodes.new("ShaderNodeVectorMath"); sdot.operation = "DOT_PRODUCT"
    nt.links.new(vt.outputs[0], sdot.inputs[0]); sdot.inputs[1].default_value = (-0.22, 0.64, -0.73)
    broad_w = math_op(nt, "MULTIPLY", smooth_step(nt, sdot.outputs["Value"], 0.80, 0.93), broad_amt)
    col = mix_rgba(nt, broad_w, col, (1.0, 0.99, 1.0, 1.0))
    sharp_w = math_op(nt, "MULTIPLY", smooth_step(nt, sdot.outputs["Value"], sheen_min, sheen_max), sheen_amt)
    col = mix_rgba(nt, sharp_w, col, (1.0, 1.0, 1.0, 1.0))

    # 4. Crisp illustrated grazing contour ink
    if edge_ink > 0:
        ink_w = math_op(nt, "MULTIPLY", smooth_step(nt, facing, 0.28, 0.06), edge_ink)
        col = mix_rgba(nt, ink_w, col, srgb_to_lin(*ink_rgb))

    nt.links.new(col, em.inputs[0])


def make_body_material(pivot_obj):
    """Luminous iridescent Sheet-B green-blue-violet pear belly + warm peach-to-lavender head."""
    C_PEACH = srgb_to_lin(254, 184, 138)
    C_LAV   = srgb_to_lin(188, 146, 232)
    C_GREEN = srgb_to_lin(172, 224, 192)  # soft pastel aqua-green belly glow (Sheet B, balanced Lab drift!)
    C_BLUE  = srgb_to_lin(150, 188, 236)  # periwinkle sky-blue mid-flank
    C_NECK  = srgb_to_lin(222, 196, 236)

    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        X, Y, Z = sep.outputs["X"], sep.outputs["Y"], sep.outputs["Z"]

        # Sheet B green -> periwinkle blue -> violet-lavender wrap around the belly!
        diag = math_op(nt, "ADD", math_op(nt, "MULTIPLY", X, 0.30), math_op(nt, "MULTIPLY", Y, 0.72))
        grn_to_blu = smooth_step(nt, diag, -0.22, -0.04)
        belly_base = mix_rgba(nt, grn_to_blu, C_GREEN, C_BLUE)
        blu_to_lav = smooth_step(nt, math_op(nt, "ADD", diag, math_op(nt, "MULTIPLY", Z, -0.20)), -0.06, 0.18)
        lower_col = mix_rgba(nt, blu_to_lav, belly_base, C_LAV)

        # Soft pearly chest bridge
        neck_band = math_op(nt, "MULTIPLY", smooth_step(nt, Z, -0.10, -0.01), smooth_step(nt, Z, 0.10, -0.01))
        lower_col = mix_rgba(nt, math_op(nt, "MULTIPLY", neck_band, 0.45), lower_col, C_NECK)

        # Head: warm glowing apricot-peach forehead fading richly into lavender across cheek frills & back
        head_diag = math_op(nt, "ADD", math_op(nt, "MULTIPLY", Y, 0.80), math_op(nt, "MULTIPLY", math_op(nt, "ABSOLUTE", X), 0.42))
        head_lav = smooth_step(nt, head_diag, 0.00, 0.22)
        head_col = mix_rgba(nt, head_lav, C_PEACH, C_LAV)

        h_fac = smooth_step(nt, Z, -0.03, 0.13)
        col = mix_rgba(nt, h_fac, lower_col, head_col)
        add_pearl_finish(nt, col, em, rim_amt=0.34, sheen_amt=0.78, sheen_min=0.945, sheen_max=0.98,
                         broad_amt=0.16, crease_dark=0.38, edge_ink=0.66)

    return make_emission_mat("body_pearl", build)


def make_crest_material(pivot_obj):
    # Warm golden-apricot flame with WARM CARAMEL ink contour (never turns dark mauve at grazing angles!)
    C_BASE = srgb_to_lin(255, 202, 118)
    C_TIP  = srgb_to_lin(248, 162, 92)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], 0.27, 0.45), C_BASE, C_TIP)
        add_pearl_finish(nt, col, em, rim_amt=0.38, sheen_amt=0.86, sheen_min=0.88, sheen_max=0.95,
                         broad_amt=0.22, crease_dark=0.22, edge_ink=0.58, ink_rgb=(176, 108, 86))
    return make_emission_mat("crest_pearl", build)


def make_horn_material(pivot_obj):
    # Lush Sheet-B lavender-violet horn (NOT a pink spike!)
    C_BASE = srgb_to_lin(222, 174, 236)
    C_TIP  = srgb_to_lin(186, 138, 230)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], 0.20, 0.31), C_BASE, C_TIP)
        add_pearl_finish(nt, col, em, rim_amt=0.40, sheen_amt=0.80, sheen_min=0.88, sheen_max=0.95,
                         broad_amt=0.20, crease_dark=0.30, edge_ink=0.76)
    return make_emission_mat("horn_pearl", build)


def make_tail_material(pivot_obj, tail_obj):
    """Plump lavender-to-sky-blue coiled tail with an UNMISTAKABLE illustrated Sheet-B spiral swirl painted in tail-local space!"""
    C_LAV  = srgb_to_lin(202, 146, 240)
    C_BLUE = srgb_to_lin(148, 192, 242)
    # Soft Sheet-B orchid-lavender spiral ink (crisp & readable at 166px, never dark enough to look like a dark plum eye!)
    C_SWIRL_INK = srgb_to_lin(134, 98, 174)
    C_SWIRL_SH  = srgb_to_lin(156, 120, 200)

    def build(nt, em):
        # 1. Base iridescent sky-blue-to-orchid-lavender gradient (world/pivot Z)
        tc_p = nt.nodes.new("ShaderNodeTexCoord"); tc_p.object = pivot_obj
        sep_p = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc_p.outputs["Object"], sep_p.inputs[0])
        base = mix_rgba(nt, smooth_step(nt, sep_p.outputs["Z"], -0.40, -0.22), C_BLUE, C_LAV)

        # 2. Crisp Sheet-B snail-shell spiral swirl line computed in tail-LOCAL (X, Z) coords (s=1.04, center cx=0.238, cz=0.138)
        tc_t = nt.nodes.new("ShaderNodeTexCoord"); tc_t.object = tail_obj
        sep_t = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc_t.outputs["Object"], sep_t.inputs[0])
        dx = math_op(nt, "SUBTRACT", sep_t.outputs["X"], 0.238)
        dz = math_op(nt, "SUBTRACT", sep_t.outputs["Z"], 0.138)
        r = math_op(nt, "SQRT", math_op(nt, "ADD", math_op(nt, "MULTIPLY", dx, dx), math_op(nt, "MULTIPLY", dz, dz)))
        theta = math_op(nt, "ARCTAN2", dz, dx)
        ang = math_op(nt, "MODULO", math_op(nt, "ADD", theta, 8.6), 6.283185)
        r_spiral = math_op(nt, "ADD", 0.025, math_op(nt, "MULTIPLY", ang, 0.0185))
        dist_line = math_op(nt, "ABSOLUTE", math_op(nt, "SUBTRACT", r, r_spiral))
        ang_mask = smooth_step(nt, ang, 5.5, 4.7)
        r_mask = math_op(nt, "MULTIPLY", smooth_step(nt, r, 0.009, 0.022), smooth_step(nt, r, 0.162, 0.130))
        line_w = math_op(nt, "MULTIPLY", smooth_step(nt, dist_line, 0.019, 0.004), math_op(nt, "MULTIPLY", ang_mask, r_mask))
        sh_w = math_op(nt, "MULTIPLY", smooth_step(nt, dist_line, 0.040, 0.011), math_op(nt, "MULTIPLY", ang_mask, r_mask))

        # Apply spiral depth shading + crisp illustrated spiral ink line!
        base = mix_rgba(nt, math_op(nt, "MULTIPLY", sh_w, 0.45), base, C_SWIRL_SH)
        base = mix_rgba(nt, math_op(nt, "MULTIPLY", line_w, 0.90), base, C_SWIRL_INK)

        add_pearl_finish(nt, base, em, rim_amt=0.40, sheen_amt=0.80, sheen_min=0.88, sheen_max=0.95,
                         broad_amt=0.20, crease_dark=0.34, edge_ink=0.82)
    return make_emission_mat("tail_pearl", build)


def make_wing_material(pivot_obj):
    # Bold Sheet-B lavender-violet bat wing with strong dark-lavender ink (NOT faint pale cyan!)
    C_LAV  = srgb_to_lin(194, 150, 236)
    C_PERI = srgb_to_lin(164, 198, 240)
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord"); tc.object = pivot_obj
        sep = nt.nodes.new("ShaderNodeSeparateXYZ"); nt.links.new(tc.outputs["Object"], sep.inputs[0])
        col = mix_rgba(nt, smooth_step(nt, sep.outputs["Z"], -0.13, 0.02), C_PERI, C_LAV)
        add_pearl_finish(nt, col, em, rim_amt=0.34, sheen_amt=0.72, sheen_min=0.88, sheen_max=0.95,
                         broad_amt=0.16, crease_dark=0.38, edge_ink=0.88)
    return make_emission_mat("wing_pearl", build)


def make_paw_material(is_forepaw=True):
    # Matching symmetric pairs (Sheet B!): both forepaws soft pearly peach-lavender, both hind feet glossy pearl-lavender!
    col_val = srgb_to_lin(234, 210, 232) if is_forepaw else srgb_to_lin(212, 188, 240)
    def build(nt, em):
        rgb = nt.nodes.new("ShaderNodeRGB"); rgb.outputs[0].default_value = col_val
        add_pearl_finish(nt, rgb.outputs[0], em, rim_amt=0.40, sheen_amt=0.86, sheen_min=0.87, sheen_max=0.94,
                         broad_amt=0.20, crease_dark=0.34, edge_ink=0.82)
    return make_emission_mat("paw_pearl" + ("_fore" if is_forepaw else "_hind"), build)


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
frills = build_frills()
crest, r_horn, l_horn = build_crest_and_horns()
wing_l = make_wing("wing_l", side=-1)
wing_r = make_wing("wing_r", side=1)
paws = build_paws_and_feet()
eyes, catchlights, smile = build_face()
tail = build_tail()

all_parts = [body, crest, r_horn, l_horn, wing_l, wing_r, tail, smile] + frills + paws + eyes + catchlights
for ob in all_parts:
    ob.parent = pivot

if args.mode in ("toon", "preview"):
    m_body = make_body_material(pivot)
    m_crest = make_crest_material(pivot)
    m_horn = make_horn_material(pivot)
    m_tail = make_tail_material(pivot, tail)
    m_wing = make_wing_material(pivot)
    m_paw_fore = make_paw_material(is_forepaw=True)
    m_paw_hind = make_paw_material(is_forepaw=False)
    m_eye = make_eye_material()
    m_white = make_emission_mat("catchlight", lambda nt, em: setattr(em.inputs[0], "default_value", (1.0, 1.0, 1.0, 1.0)))
    m_smile = make_emission_mat("smile_mat", lambda nt, em: setattr(em.inputs[0], "default_value", srgb_to_lin(76, 28, 72)))

    body.data.materials.append(m_body)
    for fr in frills: fr.data.materials.append(m_horn)
    crest.data.materials.append(m_crest)
    r_horn.data.materials.append(m_horn)
    l_horn.data.materials.append(m_horn)
    tail.data.materials.append(m_tail)
    wing_l.data.materials.append(m_wing)
    wing_r.data.materials.append(m_wing)
    paws[0].data.materials.append(m_paw_fore)
    paws[1].data.materials.append(m_paw_fore)
    paws[2].data.materials.append(m_paw_hind)
    paws[3].data.materials.append(m_paw_hind)
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
    # Pure zero-jerk sine ease-in-out (derivative = 0 at both ends, gentle 1.57x midpoint velocity!)
    sine = 0.5 * (1.0 - math.cos(math.pi * u))
    b = args.ease_blend
    return (1.0 - b) * u + b * sine


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
