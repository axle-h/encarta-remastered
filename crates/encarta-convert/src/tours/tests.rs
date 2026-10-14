//! Tests on a synthetic tour written by the fixture builders below, plus an integration test on
//! one real tour (skipped without `extracted/`).

use std::path::{Path, PathBuf};

use encarta_formats::tours::tour::Tour;
use serde_json::Value;

use super::glb;
use super::images::tests::{bmp24, cursor_1bit};

// ---- fixture writers (synthetic data only) --------------------------------------------------

#[derive(Default)]
struct X(Vec<u8>);

impl X {
    fn new() -> X {
        X(b"xof 0302bin 0032".to_vec())
    }
    fn id(&mut self, id: u16) -> &mut X {
        self.0.extend_from_slice(&id.to_le_bytes());
        self
    }
    fn name(&mut self, s: &str) -> &mut X {
        self.id(1);
        self.0.extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.0.extend_from_slice(s.as_bytes());
        self
    }
    fn string(&mut self, s: &str) -> &mut X {
        self.id(2);
        self.0.extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.0.extend_from_slice(s.as_bytes());
        self.id(0x14)
    }
    fn ints(&mut self, v: &[u32]) -> &mut X {
        self.id(6);
        self.0.extend_from_slice(&(v.len() as u32).to_le_bytes());
        v.iter().for_each(|x| self.0.extend_from_slice(&x.to_le_bytes()));
        self
    }
    fn floats(&mut self, v: &[f32]) -> &mut X {
        self.id(7);
        self.0.extend_from_slice(&(v.len() as u32).to_le_bytes());
        v.iter().for_each(|x| self.0.extend_from_slice(&x.to_le_bytes()));
        self
    }
    fn open(&mut self) -> &mut X {
        self.id(0x0A)
    }
    fn close(&mut self) -> &mut X {
        self.id(0x0B)
    }
    fn material(&mut self, name: &str, texture: Option<&str>) -> &mut X {
        self.name("Material").name(name).open().floats(&[0.5, 0.5, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        if let Some(t) = texture {
            self.name("TextureFilename").open().string(t).close();
        }
        self.close()
    }
    /// A unit quad in the plane z = `z` starting at x = `x`, D3D clockwise front face (normal +z).
    fn quad(&mut self, name: &str, material: &str, x: f32, z: f32) -> &mut X {
        self.name("Mesh").name(name).open().ints(&[4]);
        self.floats(&[x, 0.0, z, x + 1.0, 0.0, z, x + 1.0, 1.0, z, x, 1.0, z]);
        self.ints(&[1, 4, 0, 1, 2, 3]);
        self.name("MeshNormals").open().ints(&[1]).floats(&[0.0, 0.0, 1.0]).ints(&[1, 4, 0, 0, 0, 0]).close();
        self.name("MeshTextureCoords").open().ints(&[4]).floats(&[0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]).close();
        self.name("MeshMaterialList").open().ints(&[1, 1, 0]).open().name(material).close().close();
        self.close()
    }
}

fn u32s(v: &mut Vec<u8>, xs: &[u32]) {
    xs.iter().for_each(|x| v.extend_from_slice(&x.to_le_bytes()));
}

fn f32s(v: &mut Vec<u8>, xs: &[f32]) {
    xs.iter().for_each(|x| v.extend_from_slice(&x.to_le_bytes()));
}

fn name256(v: &mut Vec<u8>, s: &str) {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    b.resize(256, 0xCD);
    v.extend(b);
}

fn lights_file() -> Vec<u8> {
    let mut v = Vec::new();
    u32s(&mut v, &[3, 2]);
    // _TORCH1: in front of Wall1, reaches every corner of mesh 0 face 0.
    f32s(&mut v, &[0.5, 0.5, 1.0, 4.0]);
    u32s(&mut v, &[0xFF64_3200]);
    name256(&mut v, "_TORCH1");
    u32s(&mut v, &[1, 0, 0b1111]);
    // SUN: same place, only corner 0 of Wall1, and Wall2 (facing away → no light).
    f32s(&mut v, &[0.5, 0.5, 1.0, 4.0]);
    u32s(&mut v, &[0xFF0A_0A0A]);
    name256(&mut v, "SUN");
    u32s(&mut v, &[2, 0, 1 << 16, 0b0001, 0b1111]);
    // Paint: Wall2 face 0 corner 2.
    u32s(&mut v, &[1, 0xFF01_0203]);
    v.extend_from_slice(&2i32.to_le_bytes());
    v.extend_from_slice(&0i32.to_le_bytes());
    name256(&mut v, "Wall2");
    v
}

fn square(v: &mut Vec<u8>) {
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    for (x, z) in [(0.0f32, 0.0f32), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)] {
        f32s(v, &[x, 0.0, z]);
    }
}

fn walkmap_file() -> Vec<u8> {
    let mut v = Vec::new();
    u32s(&mut v, &[3]);
    f32s(&mut v, &[0.0, 0.0, 0.0, 4.0, 1.0, 4.0]);
    u32s(&mut v, &[4]);
    u32s(&mut v, &[4]);
    square(&mut v);
    f32s(&mut v, &[0.0, 1.0, 0.5, -0.25]);
    u32s(&mut v, &[5]);
    square(&mut v);
    let enter = b"2D._L1.VISIBLE=1;3D.Sky.VISIBLE=0;WALKMAP.LOAD=syn.3wm;SCRIPT.EVENT(UserLevelChange:QTSRefID=701599999);\xe8\x01junk";
    u32s(&mut v, &[enter.len() as u32]);
    v.extend_from_slice(enter);
    let exit = b"RUN(HIDEALL);";
    u32s(&mut v, &[exit.len() as u32]);
    v.extend_from_slice(exit);
    u32s(&mut v, &[3]);
    square(&mut v);
    f32s(&mut v, &[0.25]);
    u32s(&mut v, &[1]);
    f32s(&mut v, &[0.0, 1.0, 2.0, 3.0, 1.0]);
    u32s(&mut v, &[0]);
    u32s(&mut v, &[2]);
    square(&mut v);
    v.extend_from_slice(&2u16.to_le_bytes());
    for mesh in [0u16, 2] {
        v.extend_from_slice(&mesh.to_le_bytes());
        v.push(1);
    }
    v
}

fn lpstr(v: &mut Vec<u8>, s: &str) {
    u32s(v, &[s.len() as u32]);
    v.extend_from_slice(s.as_bytes());
}

struct El<'a> {
    name: &'a str,
    hotspot: bool,
    label: bool,
    key: bool,
    image: &'a str,
    rect: [f32; 4],
}

fn element(v: &mut Vec<u8>, e: &El) {
    u32s(v, &[6, e.hotspot as u32, 0, 0, 1, 1, (!e.label || e.hotspot) as u32, 0, e.key as u32, 1, 0xFFFFFF, 1, 0xFFFF_FFFF, 0x1234_5678]);
    f32s(v, &[0.0, 0.0, 0.0]);
    u32s(v, &[2, e.label as u32, 0xFF00FF, 0xFF00FF, 0xFF00FF, 0xC0D1E3, 0xFFFFFF, 0xFFFFFF]);
    let icon: i32 = if e.hotspot { 0 } else { -1 };
    for i in [icon, icon + 2, icon + 1] {
        v.extend_from_slice(&i.to_le_bytes());
    }
    for i in [0u16, 1, 2, 3, 0] {
        v.extend_from_slice(&i.to_le_bytes());
    }
    u32s(v, &[4]);
    let [l, t, r, b] = e.rect;
    for (x, y, u, vv) in [(l, t, 0.0, 0.0), (r, t, 1.0, 0.0), (r, b, 1.0, 1.0), (l, b, 0.0, 1.0)] {
        f32s(v, &[x, y, 1e-5, 1.0]);
        u32s(v, &[0xFFFFFF, 0]);
        f32s(v, &[u, vv]);
    }
    u32s(v, &[0xC0D1E3, 0xFFFFFF, 0xFFFFFF]);
    v.extend_from_slice(&[0; 12]);
    for i in [-11i32, 0, 0, 0, 400] {
        v.extend_from_slice(&i.to_le_bytes());
    }
    v.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0x20]);
    let mut face = b"Tahoma".to_vec();
    face.resize(32, 0);
    v.extend(face);
    lpstr(v, if e.label { "ERS Label" } else { "" });
    u32s(v, &[e.hotspot as u32]);
    for m in ["", "", "", ""] {
        lpstr(v, m);
    }
    lpstr(v, e.name);
    lpstr(v, e.image);
}

fn layer_file(elements: &[El]) -> Vec<u8> {
    let mut v = Vec::new();
    u32s(&mut v, &[4, 1]);
    lpstr(&mut v, r"\\server\share\3DVTicon.bmp");
    u32s(&mut v, &[0, elements.len() as u32]);
    elements.iter().for_each(|e| element(&mut v, e));
    v
}

fn cloud_file() -> Vec<u8> {
    let mut v = Vec::new();
    u32s(&mut v, &[3, 3]);
    for (x, y, z, k, c) in [(-1i16, 0i16, 0i16, 10.0f32, 0xF800u16), (1, 1, 2, f32::NAN, 0x07E0), (0, 2, -1, 200.0, 0x001F)] {
        for q in [x, y, z, 0] {
            v.extend_from_slice(&q.to_le_bytes());
        }
        f32s(&mut v, &[k]);
        v.extend_from_slice(&c.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
    }
    u32s(&mut v, &[1]);
    v
}

fn gif_2x4() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = gif::Encoder::new(&mut out, 2, 4, &[0, 0, 0, 255, 0, 255, 10, 20, 30]).unwrap();
        let f = gif::Frame { width: 2, height: 4, buffer: std::borrow::Cow::Borrowed(&[1, 2, 2, 2, 2, 2, 2, 0]), ..Default::default() };
        e.write_frame(&f).unwrap();
    }
    out
}

const JPEG_8X4: [u8; 17] = [0xFF, 0xD8, 0xFF, 0xC0, 0, 11, 8, 0, 4, 0, 8, 1, 1, 0x11, 0, 0xFF, 0xD9];

fn script(lines: &[&str]) -> Vec<u8> {
    lines.iter().flat_map(|l| format!("{l}\r\n").into_bytes()).collect()
}

/// Writes the synthetic tour "SYN" into a fresh directory under the build's target directory.
fn synthetic_tour(tag: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().join("tour-fixtures").join(format!("{tag}.EIT"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let w = |name: &str, data: &[u8]| std::fs::write(dir.join(name), data).unwrap();
    let mut x = X::new();
    x.name("Header").open().ints(&[1, 0, 0]).close();
    x.material("xof_default", None).material("stone", Some("wall.bmp")).material("leaf", Some("leaf.bmp")).material("sky", Some("sky.jpg"));
    x.quad("Wall1", "stone", 0.0, 0.0).quad("Wall2", "stone", 2.0, 1.0).quad("Tree", "leaf", 4.0, 2.0).quad("Sky", "sky", 6.0, 3.0);
    x.quad("Torch", "stone", 8.0, 4.0).quad("Col", "stone", 10.0, 5.0).quad("lod_Col", "stone", 12.0, 6.0).quad("Statue", "leaf", 14.0, 7.0);
    w("syn.x", &x.0);
    w("syn.3lt", &lights_file());
    w("syn.3wm", &walkmap_file());
    w("stat.3cl", &cloud_file());
    w("wall.jpg", &JPEG_8X4);
    w("sky.jpg", &JPEG_8X4);
    w("leaf.bmp", &bmp24(2, 2, &[[255, 0, 255], [10, 200, 10], [10, 200, 10], [10, 200, 10]]));
    w("fire.bmp", &bmp24(32, 1, &[[255, 0, 255]; 32]));
    w("3DVTicon.bmp", &bmp24(4, 2, &[[1, 1, 1], [2, 2, 2], [3, 3, 3], [4, 4, 4], [5, 5, 5], [6, 6, 6], [7, 7, 7], [8, 8, 8]]));
    w("anim.gif", &gif_2x4());
    w("sv.cur", &cursor_1bit());
    w("Thumbs.db", b"junk");
    w("begin.32d", &layer_file(&[El { name: "_0", hotspot: false, label: false, key: true, image: "anim.gif", rect: [0.0, 0.0, 2.0, 4.0] }]));
    w(
        "lh.32d",
        &layer_file(&[
            El { name: "_L1", hotspot: false, label: true, key: true, image: "", rect: [22.0, 34.0, 111.0, 56.0] },
            El { name: "_H1", hotspot: true, label: true, key: false, image: "", rect: [0.0, 0.0, 91.0, 22.0] },
        ]),
    );
    w(
        "load.3sc",
        &script(&[
            "'synthetic load sequence",
            "SCENE.DELETEALL",
            "CAMERA.POS=1,2,3,90",
            "CAMERA.PITCH=10",
            "2D.LOAD=begin.32d",
            "2D._0.NEXTFRAME(4)",
            "SCENE.ALARM.NEW(1,-1,0.1,LOADALARM,2d._0.nextframe(4);scene.draw)",
            "SCENE.ALARM.ACTIVATE",
            "CAMERA.HEIGHT=.1",
            "3D.OFF",
            "3D.LOAD(syn.x)",
            "SCENE.AMBIENT=20,20,20",
            "LIGHTS.LOAD(syn.3lt)",
            "WALKMAP.LOAD(syn.3wm)",
            "WALKMAP.ON",
            "2D.ICONFILE=3DVTICON.bmp",
            "2D.ICONS=2",
            "2D.LOAD(lh.32d)",
            "SCENE.LOAD(geo.3sc)",
            "SCENE.CURSOR=0,sv.cur",
            "SCENE.CURSOR=6,grip.cur",
            "SCENE.CURSORS=TRUE",
            "RUN(HIDEALL);",
            "SCENE.ALARM.LOADALARM.DELETE",
            "3D.ON",
            "SCRIPT.EVENT(WORLDLOADED:XOFFSET=1,ZOFFSET=2,XEXTENT=10,ZEXTENT=20,FIELDOFVIEW=80,SM_RECTLEFT=1,SM_RECTTOP=2,SM_RECTRIGHT=3,SM_RECTBOTTOM=4)",
        ]),
    );
    w(
        "geo.3sc",
        &script(&[
            "3D.Sky.NOILLUMINATION=1",
            "3d.torch.sprite=1",
            "3D.Torch.FILMSTRIP=16,.07,fire.bmp",
            "3D.Statue.SPRITE=1",
            "3D.Statue.CLOUDSCALED=stat.3cl,0.5",
            "3D.Statue.CLOUD.OFFSET=1.5",
            "3D.Nothing.VISIBLE=0",
            "LIGHTS._TORCH1.FLICKER=.1",
            "2D._H1.TEXT=^Big Hall^;2D._H1.ANCHORPOINT=1,2,3,45;2D._H1.ANCHORED=1;2D._H1.icon=6;2d._H1.MACRO.MOUSEUP=SCRIPT.EVENT(HotSpotClicked:hotSpotID=701500001);",
            "BEGIN(HideAll);",
            "\t2D._L1.VISIBLE=0;",
            "\t2D._H1.VISIBLE=0;",
            "'commented out",
            "\t3d.Sky.WATER=0",
            "\t3d.Missing.VISIBLE=0",
            "END(HideAll);",
        ]),
    );
    dir
}

// ---- glb reading helpers -----------------------------------------------------------------

struct Glb {
    json: Value,
    bin: Vec<u8>,
}

impl Glb {
    fn parse(b: &[u8]) -> Glb {
        let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        Glb { json: serde_json::from_slice(&b[20..20 + jl]).unwrap(), bin: b[20 + jl + 8..].to_vec() }
    }
    fn node(&self, name: &str) -> &Value {
        self.json["nodes"].as_array().unwrap().iter().find(|n| n["name"] == name).unwrap_or_else(|| panic!("no node {name}"))
    }
    fn bytes(&self, accessor: usize) -> (&[u8], &Value) {
        let a = &self.json["accessors"][accessor];
        let v = &self.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let o = v["byteOffset"].as_u64().unwrap() as usize;
        (&self.bin[o..o + v["byteLength"].as_u64().unwrap() as usize], a)
    }
    fn floats(&self, accessor: usize) -> Vec<f32> {
        self.bytes(accessor).0.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect()
    }
    fn u8s(&self, accessor: usize) -> Vec<u8> {
        self.bytes(accessor).0.to_vec()
    }
    fn indices(&self, accessor: usize) -> Vec<u32> {
        let (b, a) = self.bytes(accessor);
        match a["componentType"].as_u64().unwrap() as u32 {
            glb::UNSIGNED_SHORT => b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c) as u32).collect(),
            _ => b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect(),
        }
    }
    fn primitive(&self, node: &str, i: usize) -> &Value {
        let m = self.node(node)["mesh"].as_u64().unwrap() as usize;
        &self.json["meshes"][m]["primitives"][i]
    }
}

fn attr(p: &Value, name: &str) -> usize {
    p["attributes"][name].as_u64().unwrap_or_else(|| panic!("no {name}")) as usize
}

fn converted(tag: &str) -> (Tour, super::Converted) {
    let dir = synthetic_tour(tag);
    let tour = Tour::load(&dir).unwrap();
    let c = super::convert(&tour, "99999", None).unwrap();
    (tour, c)
}

fn scene_json(c: &super::Converted) -> Value {
    serde_json::from_slice(&c.files["scene.json"]).unwrap()
}

// ---- tests ---------------------------------------------------------------------------------

#[test]
fn synthetic_tour_passes_every_check() {
    let (_, c) = converted("checks");
    for ch in &c.report.checks {
        assert!(ch.ok, "{}: {}", ch.name, ch.detail);
    }
    assert!(glb::check(&c.files["scene.glb"]).is_ok());
    for f in ["scene.glb", "scene.json", "pvs.bin", "cursors/sv.cur.png", "images/fire.bmp.key.png", "images/3DVTicon.bmp.strip.png", "images/anim.gif.key.png"] {
        assert!(c.files.contains_key(f), "{f} missing; have {:?}", c.files.keys().collect::<Vec<_>>());
    }
}

#[test]
fn merging_keeps_addressed_meshes_separate() {
    let (_, c) = converted("merge");
    let g = Glb::parse(&c.files["scene.glb"]);
    let names: Vec<&str> = g.json["nodes"].as_array().unwrap().iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Sky", "Torch", "Col", "lod_Col", "Statue", "merged:wall.bmp", "merged:leaf.bmp", "cloud:Statue"]);
    let s = scene_json(&c);
    let meshes = s["model"]["meshes"].as_array().unwrap();
    assert_eq!(meshes[0]["node"], Value::Null, "Wall1 is merged");
    assert_eq!(meshes[1]["parts"][0][1], 2, "Wall2 follows Wall1's two triangles in the merged primitive");
    assert_eq!(meshes[3]["node"], "Sky");
    assert_eq!(s["model"]["mergeStats"]["mergedMeshes"], 3);
    assert_eq!(g.node("Col")["extras"]["lod"]["lowDetail"], "lod_Col");
    assert_eq!(g.node("lod_Col")["extras"]["lodOf"], "Col");
    assert_eq!(g.node("lod_Col")["extras"]["neverDrawnAlone"], true);
}

#[test]
fn coordinates_are_converted_and_winding_reversed() {
    let (_, c) = converted("coords");
    let g = Glb::parse(&c.files["scene.glb"]);
    // Sky: own node translated to its centre (6.5, 0.5, -3); quad at z = 3 → -3.
    assert_eq!(g.node("Sky")["translation"], serde_json::json!([6.5, 0.5, -3.0]));
    let p = g.primitive("Sky", 0);
    let pos = g.floats(attr(p, "POSITION"));
    let nrm = g.floats(attr(p, "NORMAL"));
    let ix = g.indices(p["indices"].as_u64().unwrap() as usize);
    assert_eq!(ix.len(), 6);
    let v = |i: u32| [pos[3 * i as usize], pos[3 * i as usize + 1], pos[3 * i as usize + 2]];
    let (a, b, cc) = (v(ix[0]), v(ix[1]), v(ix[2]));
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]];
    let cross = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
    let n = &nrm[3 * ix[0] as usize..3 * ix[0] as usize + 3];
    assert_eq!(n, [0.0, 0.0, -1.0], "normal z negated");
    assert!(cross[2] < 0.0 && cross[0] == 0.0 && cross[1] == 0.0, "counter-clockwise front face matches the converted normal");
    // V flipped: the disc's (0,1) at the bottom-left corner becomes (0,0), its (1,0) at the
    // top-right corner (1,1).
    let uv = g.floats(attr(p, "TEXCOORD_0"));
    let corner_uv = |want_x: f32, want_y: f32| {
        let i = (0..pos.len() / 3).find(|&i| pos[3 * i].signum() == want_x && pos[3 * i + 1].signum() == want_y).expect("corner");
        [uv[2 * i], uv[2 * i + 1]]
    };
    assert_eq!(corner_uv(-1.0, -1.0), [0.0, 0.0], "bottom-left");
    assert_eq!(corner_uv(1.0, 1.0), [1.0, 1.0], "top-right");
    assert_eq!(corner_uv(1.0, -1.0), [1.0, 0.0], "bottom-right");
    let s = scene_json(&c);
    assert_eq!(s["camera"]["start"]["position"], serde_json::json!([1.0, 2.0, -3.0]));
    assert_eq!(s["camera"]["start"]["yawDeg"], 90.0);
}

#[test]
fn lighting_is_baked_and_flicker_exported() {
    let (_, c) = converted("light");
    let g = Glb::parse(&c.files["scene.glb"]);
    let p = g.primitive("merged:wall.bmp", 0);
    let pos = g.floats(attr(p, "POSITION"));
    let col = g.u8s(attr(p, "COLOR_0"));
    let slots = g.u8s(attr(p, "_FLICKER_SLOTS"));
    let weights = g.floats(attr(p, "_FLICKER_WEIGHTS"));
    let find = |x: f32, y: f32, z: f32| (0..pos.len() / 3).find(|&i| pos[3 * i..3 * i + 3] == [x, y, z]).unwrap();
    // Wall1 corner 0 at (0,0,0): ambient 20 + trunc(10 · f), f = (1 − 1.5/4)·(1/√1.5) ≈ 0.51 → 25.
    let i0 = find(0.0, 0.0, 0.0);
    assert_eq!(col[4 * i0..4 * i0 + 4], [25, 25, 25, 255]);
    assert_eq!(slots[4 * i0], 0);
    assert!((weights[4 * i0] - 0.5103).abs() < 1e-3, "flicker weight {}", weights[4 * i0]);
    // Wall1 corner 1 at (1,0,0): SUN's mask excludes it → ambient only, still flickers.
    let i1 = find(1.0, 0.0, 0.0);
    assert_eq!(col[4 * i1..4 * i1 + 3], [20, 20, 20]);
    assert!(weights[4 * i1] > 0.0);
    // Wall2 (z = 1 → -1) faces away from the light: ambient, except the painted corner 2.
    let painted = find(3.0, 1.0, -1.0);
    assert_eq!(col[4 * painted..4 * painted + 3], [1, 2, 3]);
    assert_eq!(col[4 * find(2.0, 0.0, -1.0)..][..3], [20, 20, 20]);
    // NOILLUMINATION and SPRITE objects are full-bright.
    let sky = g.u8s(attr(g.primitive("Sky", 0), "COLOR_0"));
    assert!(sky.iter().all(|&b| b == 255));
    let s = scene_json(&c);
    assert_eq!(s["lights"]["flickerSlots"][0]["name"], "_TORCH1");
    assert_eq!(s["lights"]["lights"][0]["flicker"]["period"], 0.1);
    assert_eq!(s["lights"]["lights"][1]["baked"], true);
}

#[test]
fn textures_follow_the_stem_and_key_rules() {
    let (_, c) = converted("tex");
    let s = scene_json(&c);
    let mats = s["model"]["materials"].as_array().unwrap();
    let wall = mats.iter().find(|m| m["textureName"] == "wall.bmp").unwrap();
    assert_eq!((wall["textureFile"].as_str(), wall["resolution"].as_str(), wall["keyed"].as_bool()), (Some("wall.jpg"), Some("JpgFallback"), Some(false)));
    let leaf = mats.iter().find(|m| m["textureName"] == "leaf.bmp").unwrap();
    assert_eq!(leaf["keyed"], true);
    let g = Glb::parse(&c.files["scene.glb"]);
    let gm = &g.json["materials"][leaf["index"].as_u64().unwrap() as usize];
    assert_eq!(gm["alphaMode"], "MASK");
    assert!(gm["extensions"]["KHR_materials_unlit"].is_object());
    let img = &g.json["images"][g.json["textures"][leaf["gltfTexture"].as_u64().unwrap() as usize]["source"].as_u64().unwrap() as usize];
    assert_eq!(img["mimeType"], "image/png");
    let wimg = &g.json["images"][g.json["textures"][wall["gltfTexture"].as_u64().unwrap() as usize]["source"].as_u64().unwrap() as usize];
    assert_eq!(wimg["mimeType"], "image/jpeg");
    assert_eq!(g.json["samplers"][1]["minFilter"], 9729, "keyed textures are not mipmapped");
}

#[test]
fn scripts_become_structured_state_and_actions() {
    let (_, c) = converted("script");
    let s = scene_json(&c);
    let torch = s["objects"].as_array().unwrap().iter().find(|o| o["node"] == "Torch").unwrap();
    assert_eq!(torch["state"]["sprite"], true);
    assert_eq!(torch["state"]["filmstrip"], serde_json::json!([16, 0.07]));
    assert_eq!(torch["filmstripFrames"], 2);
    let sky = s["objects"].as_array().unwrap().iter().find(|o| o["node"] == "Sky").unwrap();
    assert_eq!(sky["state"]["noIllumination"], true);
    // The glued HIDEALL statement: only WATER=0 on Sky survives (§3.1).
    let hide = s["macros"].as_array().unwrap().iter().find(|m| m["name"] == "HIDEALL").unwrap();
    assert_eq!(hide["runDuringLoad"], true);
    let last = hide["steps"].as_array().unwrap().last().unwrap();
    assert_eq!((last["op"].as_str(), last["prop"].as_str(), last["target"].as_str(), last["value"].as_bool()), (Some("objectFlag"), Some("water"), Some("Sky"), Some(false)));
    assert_eq!(last["src"], "3d.Sky.WATER=03d.Missing.VISIBLE=0");
    // 2D state after load: text, anchor (z negated), icon; hidden by HIDEALL.
    let lh = s["layers"].as_array().unwrap().iter().find(|l| l["file"] == "lh.32d").unwrap();
    let h1 = lh["elements"].as_array().unwrap().iter().find(|e| e["name"] == "_H1").unwrap();
    assert_eq!(h1["state"]["text"], "Big Hall");
    assert_eq!(h1["state"]["anchor"], serde_json::json!([1.0, 2.0, -3.0]));
    assert_eq!((h1["state"]["icon"].as_i64(), h1["state"]["visible"].as_bool(), h1["state"]["anchored"].as_bool()), (Some(6), Some(false), Some(true)));
    let begin = s["layers"].as_array().unwrap().iter().find(|l| l["file"] == "begin.32d").unwrap();
    assert_eq!(begin["elements"][0]["state"]["uv"], serde_json::json!([0.0, 0.0, 1.0, 0.25]), "NEXTFRAME(4) from the whole image shows frame 0");
    assert_eq!(begin["elements"][0]["image"], "images/anim.gif.key.png");
    // Hotspot link and its event.
    assert_eq!(s["hotspots"][0]["contentId"], 701500001u64);
    assert_eq!(s["hotspots"][0]["title"], "Big Hall");
    // Alarms, events, site map, cursors.
    let alarms = s["alarms"].as_array().unwrap();
    assert_eq!((alarms[0]["name"].as_str(), alarms[0]["live"].as_bool(), alarms[0]["macroText"].as_str()), (Some("LOADALARM"), Some(false), Some("2D._0.NEXTFRAME(4")));
    assert_eq!(s["siteMap"]["xOffset"], 1.0);
    assert_eq!(s["siteMap"]["rect"]["bottom"], 4);
    let slots = s["cursors"]["slots"].as_array().unwrap();
    assert_eq!(slots[0]["image"], "cursors/sv.cur.png");
    assert_eq!(slots[1]["image"], Value::Null, "grip.cur is missing");
    assert_eq!(s["cursors"]["files"]["sv.cur"]["hotspot"], serde_json::json!([1, 0]));
    // Unknown ids keep a null target.
    let nothing = s["loadSequence"].as_array().unwrap().iter().find(|e| e["src"] == "3D.Nothing.VISIBLE=0").unwrap();
    assert_eq!(nothing["target"], Value::Null);
    assert_eq!(nothing["origin"], "geo.3sc:7");
    let omitted: Vec<&str> = s["omitted"].as_array().unwrap().iter().map(|o| o["file"].as_str().unwrap()).collect();
    assert!(omitted.contains(&"Thumbs.db"));
}

#[test]
fn walk_maps_hotspots_and_pvs() {
    let (_, c) = converted("walk");
    let s = scene_json(&c);
    let m = &s["walkmaps"]["maps"][0];
    let cells = m["cells"].as_array().unwrap();
    assert_eq!(cells[0]["plane"], serde_json::json!([0.0, 1.0, -0.5, -0.25]));
    assert_eq!(m["bbox"]["min"], serde_json::json!([0.0, 0.0, -4.0]));
    let enter = &cells[1]["enter"];
    assert_eq!(enter["junkBytes"], 6);
    let ops: Vec<&str> = enter["steps"].as_array().unwrap().iter().map(|s| s["op"].as_str().unwrap()).collect();
    assert_eq!(ops, ["elementVisible", "objectFlag", "walkmapLoad", "event"]);
    assert_eq!(cells[1]["exit"]["steps"][0]["op"], "runMacros");
    assert_eq!(cells[2]["segments"][0], serde_json::json!([0.0, -1.0, 2.0, -3.0, 1.0, 0]));
    assert_eq!(s["walkmaps"]["floors"][0]["qtsRefIds"][0], "701599999");
    // PVS: Wall1 face 0 and Tree face 0, each 2 triangles in different merged units.
    let pvs = &cells[3]["pvs"];
    assert_eq!((pvs["runCount"].as_u64(), pvs["faces"].as_u64()), (Some(2), Some(2)));
    let bin = &c.files["pvs.bin"];
    assert_eq!(bin.len(), 16);
    let units = s["model"]["drawUnits"].as_array().unwrap();
    let unit = |i: usize| u16::from_le_bytes(bin[8 * i + 4..8 * i + 6].try_into().unwrap()) as usize;
    let count = |i: usize| u16::from_le_bytes(bin[8 * i + 6..8 * i + 8].try_into().unwrap());
    assert_eq!(units[unit(0)]["node"], "merged:wall.bmp");
    assert_eq!(units[unit(1)]["node"], "merged:leaf.bmp");
    assert_eq!((count(0), count(1)), (2, 2));
}

#[test]
fn clouds_become_point_primitives() {
    let (_, c) = converted("cloud");
    let g = Glb::parse(&c.files["scene.glb"]);
    let n = g.node("cloud:Statue");
    assert_eq!(n["matrix"].as_array().unwrap().len(), 16);
    let p = g.primitive("cloud:Statue", 0);
    assert_eq!(p["mode"], glb::MODE_POINTS);
    assert_eq!(g.floats(attr(p, "POSITION")), [-1.0, 0.0, -0.0, 1.0, 1.0, -2.0, 0.0, 2.0, 1.0]);
    assert_eq!(g.floats(attr(p, "_KEY")), [10.0, -1.0, 200.0], "NaN keys become -1");
    assert_eq!(g.u8s(attr(p, "COLOR_0"))[..4], [255, 0, 0, 255]);
    let s = scene_json(&c);
    assert_eq!(s["clouds"][0]["scale"], 0.5);
    assert_eq!(s["clouds"][0]["offsetRadians"], 1.5);
    assert_eq!(s["clouds"][0]["textureSize"], serde_json::json!([2, 2]));
}

#[test]
fn conversion_is_deterministic() {
    let (tour, a) = converted("determinism");
    let b = super::convert(&tour, "99999", None).unwrap();
    assert_eq!(a.files.keys().collect::<Vec<_>>(), b.files.keys().collect::<Vec<_>>());
    for (k, v) in &a.files {
        assert!(&b.files[k] == v, "{k} differs between runs");
    }
    // Writing twice leaves the same tree and removes stale files.
    let out = std::env::current_exe().unwrap().parent().unwrap().join("tour-fixtures/out-determinism");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(out.join("images")).unwrap();
    std::fs::write(out.join("images/stale.png"), b"x").unwrap();
    super::write_dir(&out, &a.files).unwrap();
    super::write_dir(&out, &b.files).unwrap();
    assert!(!out.join("images/stale.png").exists());
    assert_eq!(std::fs::read(out.join("scene.json")).unwrap(), a.files["scene.json"]);
}

/// Integration test on Abu Simbel (05919), the smallest tour; skipped without `extracted/`.
#[test]
fn abu_simbel_from_the_disc() {
    let Some(dir) = super::extracted_tour("05919") else {
        eprintln!("skipped: extracted/ not present");
        return;
    };
    let tour = Tour::load(Path::new(&dir)).unwrap();
    let c = super::convert(&tour, "05919", None).unwrap();
    for ch in &c.report.checks {
        assert!(ch.ok, "{}: {}", ch.name, ch.detail);
    }
    let s = scene_json(&c);
    assert_eq!(s["tour"]["contentId"], 701505919u64);
    assert_eq!(s["model"]["meshes"].as_array().unwrap().len(), 531);
    assert_eq!(s["lights"]["lights"].as_array().unwrap().len(), 128);
    assert_eq!(s["lights"]["flickerSlots"].as_array().unwrap().len(), 57, "§7.2.6: 57 of 59 FLICKER statements hit a light");
    assert_eq!(s["scene"]["ambient"], serde_json::json!([0, 0, 0]));
    assert_eq!(s["hotspots"].as_array().unwrap().len(), 18);
    assert_eq!(s["walkmaps"]["start"], "abu.3wm");
    assert_eq!(s["walkmaps"]["maps"][0]["cells"].as_array().unwrap().len(), 618);
    assert_eq!(s["camera"]["height"], 0.37);
    assert_eq!(s["siteMap"]["xExtent"], 21.02812);
    assert_eq!(c.report.glb.triangles, 55_494);
    let layer = s["layers"].as_array().unwrap().iter().find(|l| l["final"] == true).unwrap();
    assert_eq!(layer["elements"].as_array().unwrap().len(), 57);
}

#[test]
fn content_record_adds_stops_and_hotspot_destinations() {
    use super::content::{HotspotTarget, Site, SiteLocation, Stop, TourContent};
    let dir = synthetic_tour("content");
    let tour = Tour::load(&dir).unwrap();
    // A 4×2 site map: left half plain, right half tinted (synthetic pixels).
    let map = super::images::tests::bmp24(4, 2, &[[255, 255, 255], [200, 200, 200], [250, 220, 200], [190, 170, 150], [255, 255, 255], [0, 0, 255], [250, 220, 200], [0, 0, 200]]);
    let record = TourContent {
        title: Some("Test Tour, 100 BC".into()),
        default_stop: Some(11),
        stops: vec![Stop {
            id: 11,
            title: "Hall".into(),
            command: Some("CAMERA.MOVETO=1,0,2,90;".into()),
            hotspots: vec![701500001],
            site: Some(21),
            article: Some(31),
        }],
        article: Some(41),
        sites: vec![Site {
            id: 21,
            image_file: Some("map21.dib".into()),
            locations: vec![SiteLocation { id: 22, title: "Hall".into(), stop: Some(11), x: Some(1), y: Some(0) }],
            image_data: Some(map),
        }],
        hotspots: vec![HotspotTarget {
            id: 701500001,
            title: "Big Hall".into(),
            dest_media: Some(681500999),
            dest_group: Some("media".into()),
            dest_kind: Some("media".into()),
            dest_title: Some("Big Hall".into()),
            stops: vec![11],
        }],
    };
    let c = super::convert(&tour, "99999", Some(&record)).unwrap();
    for ch in &c.report.checks {
        assert!(ch.ok, "{}: {}", ch.name, ch.detail);
    }
    let s = scene_json(&c);
    assert_eq!(s["defaultStop"], 11);
    assert_eq!(s["stops"][0]["command"], "CAMERA.MOVETO=1,0,2,90;");
    let h = s["hotspots"].as_array().unwrap().iter().find(|h| h["contentId"] == 701500001).unwrap();
    assert_eq!(h["destMedia"], 681500999);
    assert_eq!(h["destGroup"], "media");
    assert_eq!(s["hotspotTargets"][0]["destMedia"], 681500999);
    assert_eq!((s["stops"][0]["site"].as_u64(), s["stops"][0]["article"].as_u64(), s["article"].as_u64()), (Some(21), Some(31), Some(41)));
    let site = &s["sites"][0];
    assert_eq!(s["tour"]["title"], "Test Tour, 100 BC");
    assert_eq!(site["image"], "images/sitemap-21.png");
    assert_eq!(site["size"], serde_json::json!([4, 2]));
    assert_eq!(site["locations"][0]["stop"], 11);
    let png = super::images::decode(&c.files["images/sitemap-21.png"]).unwrap();
    assert_eq!((png.pixels[0], png.pixels[5]), ([255, 255, 255, 255], [0, 0, 255, 255]), "display orientation, top row first");

    // A record that lacks the element's hotspot fails the destination check.
    let empty = TourContent::default();
    let c = super::convert(&tour, "99999", Some(&empty)).unwrap();
    assert!(c.report.checks.iter().any(|ch| ch.name == "hotspot destinations" && !ch.ok));
    // A site whose image was not found fails the site-map check.
    let mut no_image = record.clone();
    no_image.sites[0].image_data = None;
    let c = super::convert(&tour, "99999", Some(&no_image)).unwrap();
    assert!(c.report.checks.iter().any(|ch| ch.name == "site maps" && !ch.ok));
    // No record: no stops, null destinations, check skipped.
    let c = super::convert(&tour, "99999", None).unwrap();
    let s = scene_json(&c);
    assert_eq!(s["stops"], serde_json::json!([]));
    assert_eq!(s["sites"], serde_json::json!([]));
    assert!(s["defaultStop"].is_null());
}
