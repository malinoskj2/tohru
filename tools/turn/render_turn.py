"""Blender: one head/body turn of the dragon mesh, rendered as B1 toon frames or B2 guide passes.

blender -b -P render_turn.py -- MESH.glb OUT MODE [--yaw0 90 --yaw1 0 --hold 6 --turn 14 --size 480 --at YAW]
MODE: toon, position (object-space coordinates), normal (camera-space normals), onormal (object-space normals), alpha.
--at renders one still at a fixed yaw instead of the animation (for key registration).
"""
import argparse
import os
import math
import sys

import bpy
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:]
ap = argparse.ArgumentParser()
ap.add_argument("mesh")
ap.add_argument("out")
ap.add_argument("mode")
ap.add_argument("--yaw0", type=float, default=90)
ap.add_argument("--yaw1", type=float, default=0)
ap.add_argument("--hold", type=int, default=6)
ap.add_argument("--turn", type=int, default=14)
ap.add_argument("--dip", type=float, default=5)
ap.add_argument("--size", type=int, default=480)
ap.add_argument("--at", type=float, default=None)
ap.add_argument("--keep-origin", action="store_true", help="the mesh is already in render units around its axis")
ap.add_argument("--depth16", action="store_true")
ap.add_argument("--base-yaw", type=float, default=0, help="yaw that makes the mesh face the camera")
args = ap.parse_args(argv)

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=args.mesh)
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
pts = [obj.matrix_world @ v.co for v in obj.data.vertices]
lo = Vector([min(p[i] for p in pts) for i in range(3)])
hi = Vector([max(p[i] for p in pts) for i in range(3)])
size = max(hi - lo)
if not args.keep_origin:
    for v in obj.data.vertices:
        v.co = (v.co - (lo + hi) / 2) / size
bpy.ops.object.shade_smooth()

pivot = bpy.data.objects.new("pivot", None)
bpy.context.scene.collection.objects.link(pivot)
obj.parent = pivot

scene = bpy.context.scene
scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = scene.render.resolution_y = args.size
scene.render.film_transparent = True
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGBA"
if args.depth16:
    scene.render.image_settings.color_depth = "16"
scene.view_settings.view_transform = "Raw" if args.mode in ("position", "normal", "onormal") else "Standard"
scene.render.filter_size = 1.0

cam_data = bpy.data.cameras.new("cam")
cam_data.type = "ORTHO"
cam_data.ortho_scale = 1.25
cam = bpy.data.objects.new("cam", cam_data)
scene.collection.objects.link(cam)
# glTF imports Y-up as Blender Z-up; the camera looks along +Y at the front (-Y facing) side.
cam.location = (0, -5, 0)
cam.rotation_euler = (math.radians(90), 0, 0)
scene.camera = cam


def emission_material(name, build):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(em.outputs[0], out.inputs[0])
    build(nt, em)
    return mat


def sock(sockets, name):
    """Mix nodes carry float, vector and colour sockets under one name; pick the colour one."""
    for x in sockets:
        if x.name == name and x.type == "RGBA" and x.enabled:
            return x
    return sockets[name]


def set_material(mat):
    obj.data.materials.clear()
    obj.data.materials.append(mat)


original = [s.material for s in obj.material_slots] if obj.material_slots else []

if args.mode == "position":
    def build(nt, em):
        tc = nt.nodes.new("ShaderNodeTexCoord")
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Location"].default_value = (0.5, 0.5, 0.5)
        nt.links.new(tc.outputs["Object"], mp.inputs[0])
        nt.links.new(mp.outputs[0], em.inputs[0])
    set_material(emission_material("pos", build))
elif args.mode == "normal":
    def build(nt, em):
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        vt = nt.nodes.new("ShaderNodeVectorTransform")
        vt.vector_type = "NORMAL"
        vt.convert_from = "WORLD"
        vt.convert_to = "CAMERA"
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Location"].default_value = (0.5, 0.5, 0.5)
        mp.inputs["Scale"].default_value = (0.5, 0.5, 0.5)
        nt.links.new(geo.outputs["Normal"], vt.inputs[0])
        nt.links.new(vt.outputs[0], mp.inputs[0])
        nt.links.new(mp.outputs[0], em.inputs[0])
    set_material(emission_material("nrm", build))
elif args.mode == "onormal":
    def build(nt, em):
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        vt = nt.nodes.new("ShaderNodeVectorTransform")
        vt.vector_type, vt.convert_from, vt.convert_to = "NORMAL", "WORLD", "OBJECT"
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Location"].default_value = (0.5, 0.5, 0.5)
        mp.inputs["Scale"].default_value = (0.5, 0.5, 0.5)
        nt.links.new(geo.outputs["Normal"], vt.inputs[0])
        nt.links.new(vt.outputs[0], mp.inputs[0])
        nt.links.new(mp.outputs[0], em.inputs[0])
    set_material(emission_material("onrm", build))
elif args.mode == "alpha":
    set_material(emission_material("white", lambda nt, em: setattr(em.inputs[0], "default_value", (1, 1, 1, 1))))
elif args.mode == "toon":
    # Unlit painted albedo with soft view-facing shading, a pearl rim, a sheen band and a coloured ink outline.
    tex_img = None
    for m in original:
        if m and m.use_nodes:
            for n in m.node_tree.nodes:
                if n.type == "TEX_IMAGE" and n.image:
                    tex_img = n.image
    print("TOON texture", tex_img, flush=True)
    def build(nt, em):
        def vm(op, a, b=None, value=None):
            n = nt.nodes.new("ShaderNodeVectorMath"); n.operation = op
            for i, x in enumerate((a, b)):
                if x is None:
                    continue
                if isinstance(x, tuple):
                    n.inputs[i].default_value = x
                else:
                    nt.links.new(x, n.inputs[i])
            if value is not None:
                n.inputs[3].default_value = value
            return n.outputs[0]

        def m(op, a, b):
            n = nt.nodes.new("ShaderNodeMath"); n.operation = op
            for i, x in enumerate((a, b)):
                if isinstance(x, (int, float)):
                    n.inputs[i].default_value = x
                else:
                    nt.links.new(x, n.inputs[i])
            return n.outputs[0]

        geo = nt.nodes.new("ShaderNodeNewGeometry")
        vt = nt.nodes.new("ShaderNodeVectorTransform")
        vt.vector_type, vt.convert_from, vt.convert_to = "NORMAL", "WORLD", "CAMERA"
        nt.links.new(geo.outputs["Normal"], vt.inputs[0])
        sep = nt.nodes.new("ShaderNodeSeparateXYZ")
        nt.links.new(vt.outputs[0], sep.inputs[0])
        lw = nt.nodes.new("ShaderNodeLayerWeight"); lw.inputs["Blend"].default_value = 0.5
        facing = m("SUBTRACT", 1.0, lw.outputs["Facing"])
        if tex_img:
            tex = nt.nodes.new("ShaderNodeTexImage"); tex.image = tex_img
            base = tex.outputs["Color"]
        else:
            rgb = nt.nodes.new("ShaderNodeRGB"); rgb.outputs[0].default_value = (0.8, 0.75, 0.9, 1); base = rgb.outputs[0]
        shade = m("ADD", m("MULTIPLY", facing, 0.12), 0.9)
        sc = nt.nodes.new("ShaderNodeVectorMath"); sc.operation = "SCALE"
        nt.links.new(base, sc.inputs[0]); nt.links.new(shade, sc.inputs[3])
        col = sc.outputs[0]
        # Screen in a pearl rim: col + (1 - col) * rim.
        rim = m("MULTIPLY", m("POWER", m("SUBTRACT", 1.0, facing), 3.0), 0.40)
        inv = vm("SUBTRACT", (1.0, 1.0, 1.0), col)
        rs = nt.nodes.new("ShaderNodeVectorMath"); rs.operation = "SCALE"
        nt.links.new(inv, rs.inputs[0]); nt.links.new(rim, rs.inputs[3])
        col = vm("ADD", col, rs.outputs[0])
        # Glossy sheen toward an upper-left key light.
        dot = nt.nodes.new("ShaderNodeVectorMath"); dot.operation = "DOT_PRODUCT"
        nt.links.new(vt.outputs[0], dot.inputs[0]); dot.inputs[1].default_value = (-0.45, 0.6, 0.66)
        mr = nt.nodes.new("ShaderNodeMapRange"); mr.inputs["From Min"].default_value = 0.88; mr.inputs["From Max"].default_value = 0.98
        nt.links.new(dot.outputs["Value"], mr.inputs["Value"])
        sheen = m("MULTIPLY", mr.outputs["Result"], 0.0)
        inv2 = vm("SUBTRACT", (1.0, 1.0, 1.0), col)
        ss = nt.nodes.new("ShaderNodeVectorMath"); ss.operation = "SCALE"
        nt.links.new(inv2, ss.inputs[0]); nt.links.new(sheen, ss.inputs[3])
        col = vm("ADD", col, ss.outputs[0])
        nt.links.new(col, em.inputs[0])
        if os.environ.get("RAW"):
            nt.links.new(base, em.inputs[0])
    set_material(emission_material("toon", build))
    # Inverted-hull outline in soft lilac.
    ink = emission_material("ink", lambda nt, em: setattr(em.inputs[0], "default_value", (0.55, 0.45, 0.68, 1)))
    ink.use_backface_culling = True
    obj.data.materials.append(ink)
    sol = obj.modifiers.new("outline", "SOLIDIFY")
    sol.thickness = -0.006
    sol.use_flip_normals = True
    sol.material_offset = 1
    sol.use_rim = False
else:
    raise SystemExit(f"unknown mode {args.mode}")

scene.world = bpy.data.worlds.new("w")
scene.world.color = (0, 0, 0)


def ease(u):
    return u * u * (3 - 2 * u)


def pose(yaw, dip):
    pivot.rotation_euler = (math.radians(dip), 0, math.radians(args.base_yaw + yaw))


if args.at is not None:
    pose(args.at, 0)
    scene.render.filepath = args.out
    bpy.ops.render.render(write_still=True)
else:
    total = args.hold * 2 + args.turn
    import json
    os.makedirs(args.out, exist_ok=True)
    yaws = [args.yaw0 + (args.yaw1 - args.yaw0) * ease(min(1.0, max(0.0, (f - args.hold) / max(1, args.turn - 1))))
            for f in range(total)]
    with open(f"{args.out}/../yaws.json", "w") as fh:
        json.dump(yaws, fh)
    for f in range(total):
        u = min(1.0, max(0.0, (f - args.hold) / max(1, args.turn - 1)))
        e = ease(u)
        yaw = args.yaw0 + (args.yaw1 - args.yaw0) * e
        dip = args.dip * math.sin(math.pi * u)
        pose(yaw, dip)
        scene.render.filepath = f"{args.out}/{f:04d}.png"
        bpy.ops.render.render(write_still=True)
