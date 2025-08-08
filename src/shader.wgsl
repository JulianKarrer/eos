
// ------------- UNIFORMS -------------
@group(0) @binding(0) var<uniform> u: Uniforms;
struct Uniforms {
	res: vec2f,
	top_left: vec2f,
	t: f32,
	cpu_u: f32,
	cpu_m: f32,
	r: f32,
	g: f32,
	b: f32,
	a: f32,
	// buff: f32,
}
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

// ---------- VERTEX CREATION ----------
struct VertexIn {@builtin(vertex_index) vertex_index: u32,}
struct VertexOut {@builtin(position) position: vec4f,}
@vertex
fn vs_main(in: VertexIn) -> VertexOut {
	let uv = vec2f(vec2u((in.vertex_index << 1) & 2, in.vertex_index & 2));
	let position = vec4f(uv * 2. - 1., 0., 1.);
	return VertexOut(position);
}


// ------------ MAIN PROGRAM -----------
// PARAMETERS
const RADIUS:f32 = 2.0;
// OTHER CONSTANTS
const CAMERA_O:vec3f = vec3f(0.,0.,8.);
const EPS: f32 = 1e-4;
const INF: f32 = 1e20;
const RADIUS_SQ:f32 = RADIUS*RADIUS;
const PI:f32 = 3.1415926535;

struct Ray {
	o: vec3f, 
	d: vec3f,
} 

fn ray_at(r:Ray, t:f32)->vec3f{ return r.o + t*r.d;}
fn abort(p:vec3f)->bool{ 
	return p.z < -2.; 
}


@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
    // screen -> image-plane coordinates (using your uniforms)
    let ndc = 2.0 * (in.position.xy - u.top_left) / u.res - vec2f(1.0);
    let uv = vec2f(1.0, -1.0) * ndc;
    let z_plane: f32 = 5.0;

    let plane_pt_world = vec3f(uv.x, uv.y, z_plane);
    let r: Ray = Ray(CAMERA_O, normalize(plane_pt_world - CAMERA_O));

    let C: vec3f = vec3f(0.0, 0.0, 0.0);
    let L: vec3f = C - r.o;
    let tca: f32 = dot(L, r.d);
    let d2: f32 = dot(L, L) - tca * tca;

    // MISS: outside sphere -> compute halo using distance + angle
    if (d2 > RADIUS_SQ) {
        // Euclidean distance from ray to sphere center line
        let dist_line_to_center: f32 = sqrt(d2);
        let dist_to_surface: f32 = dist_line_to_center - RADIUS; // >=0

        // Project sphere center to image plane at z_plane
        let dir_to_center = normalize(L);

        // compute center_plane and silhouette_radius robustly (preserve sign)
        var center_plane = vec3f(0.0);
        var silhouette_radius: f32 = 0.0;
        var plane_dist_to_silhouette: f32 = 1e6;

        if (abs(dir_to_center.z) > 1e-6) {
            let t_plane_center = (z_plane - CAMERA_O.z) / dir_to_center.z;
            center_plane = CAMERA_O + dir_to_center * t_plane_center;

            let dist_cam_to_center: f32 = length(L);
            let denom = max(dist_cam_to_center * dist_cam_to_center - RADIUS * RADIUS, 1e-8);
            let tan_alpha = RADIUS / sqrt(denom);
            silhouette_radius = abs(t_plane_center) * tan_alpha; // radius magnitude

            // distance from pixel point to silhouette center
            let plane_vec = plane_pt_world.xy - center_plane.xy;
            let plane_dist = length(plane_vec);
            plane_dist_to_silhouette = max(0.0, plane_dist - silhouette_radius);
        } else {
            plane_dist_to_silhouette = 1e6;
        }

        // compute angle around silhouette (0 = up, increases clockwise)
        var angle_frac: f32 = 0.0;
        let plane_vec_tmp = plane_pt_world.xy - center_plane.xy;
        let plane_dist_tmp = length(plane_vec_tmp);
        if (plane_dist_tmp > 1e-6) {
            let ang = atan2(plane_vec_tmp.x, plane_vec_tmp.y); // radians in (-PI,PI]
            angle_frac = ang / (2.0 * PI);
            if (angle_frac < 0.0) { angle_frac = angle_frac + 1.0; }
        }

        // ----- Parameters (tweak these) -----
        let halo_col: vec3f = vec3f(235., 155., 0.)/255.;
        let halo_int: f32 = 1.0;
        let halo_amp: f32 = 0.2+u.cpu_u*0.5;
        let halo_freq: f32 = 0.2;

        // base glow width (image-plane units) — keep this small (0.02..0.12)
        let width_base: f32 = 0.04;

        // primary (cardinal) spikes
        let primary_count: i32 = 4;                // 0, π/2, π, 3π/2
        let primary_halfwidth: f32 = PI * 0.055;   // angular half-width (~9.9°)
        let primary_strength: f32 = 1.0;
        let primary_sharp: f32 = 2.2;              // >1 for pointier tip

        // secondary in-between spikes
        let secondary_count: i32 = 4;              // gives four between cardinals by default
        let secondary_halfwidth: f32 = PI * 0.03;  // smaller angular half-width
        let secondary_strength: f32 = 0.45;
        let secondary_sharp: f32 = 2.0;

        // spike radial behaviour
        let spike_length_factor: f32 = 4.0;        // how much longer spikes are than base glow
        let width_spike: f32 = max(0.4 * width_base, 0.001);

        // ----- Convert angle_frac [0,1) to radians -----
        let angle = angle_frac * 2.0 * PI; // THIS FIXES THE TOP DISCONTINUITY

        // ----- Inline shortest-angle difference and triangular mask -----
        var primary_acc: f32 = 0.0;
        for (var i: i32 = 0; i < primary_count; i = i + 1) {
            let center = f32(i) * (PI * 0.5); // radians: 0, π/2, π, 3π/2
            // shortest angular difference (a-b) wrapped to [-PI,PI]
            var d = angle - center;
            if (d > PI) { d = d - 2.0 * PI; }
            if (d < -PI) { d = d + 2.0 * PI; }
            // triangular mask
            var tri = clamp(1.0 - abs(d) / primary_halfwidth, 0.0, 1.0);
            if (primary_sharp > 1.0) { tri = pow(tri, primary_sharp); }
            primary_acc = primary_acc + tri;
        }

        var secondary_acc: f32 = 0.0;
        if (secondary_count > 0) {
            // put secondaries halfway between cardinals (π/4 offset)
            let offset = PI * 0.25;
            for (var j: i32 = 0; j < secondary_count; j = j + 1) {
                let center = offset + (2.0 * PI) * (f32(j) / f32(secondary_count));
                var d2 = angle - center;
                if (d2 > PI) { d2 = d2 - 2.0 * PI; }
                if (d2 < -PI) { d2 = d2 + 2.0 * PI; }
                var tri2 = clamp(1.0 - abs(d2) / secondary_halfwidth, 0.0, 1.0);
                if (secondary_sharp > 1.0) { tri2 = pow(tri2, secondary_sharp); }
                secondary_acc = secondary_acc + tri2;
            }
        }

        let ang_mod = primary_strength * primary_acc + secondary_strength * secondary_acc;
        let ang_mod_clamped = clamp(ang_mod, 0.0, 8.0);

        // radial falloffs
        let rnorm_base = plane_dist_to_silhouette / width_base;
        let base_falloff = exp(-rnorm_base * rnorm_base * 0.69314718);

        let rnorm_spike = plane_dist_to_silhouette / (width_spike * spike_length_factor);
        let spike_falloff = exp(-rnorm_spike * rnorm_spike * 0.25);

        let spike_contrib = ang_mod_clamped * spike_falloff;

        let base_strength: f32 = 1.0;
        let spike_boost: f32 = 1.8;

        let combined = base_falloff * base_strength + spike_contrib * spike_boost;

        let pulse = 1.0 + halo_amp * sin(2.0 * PI * halo_freq * u.t);
        let glow = combined * pulse * halo_int;

        let surface_fall = 1.0 / (1.0 + dist_to_surface * 4.0);

        let bg: vec3f = vec3f(u.r, u.g, u.b);
        let out_col = bg + halo_col * glow * surface_fall;
        let final_col = clamp(out_col, vec3f(0.0), vec3f(1.0));

        return vec4f(final_col, u.a);
    }

    // HIT: unchanged hit logic (sphere sampling)
    let thc: f32 = sqrt(max(RADIUS_SQ - d2, 0.0));
    let t0: f32 = tca - thc;
    let t1: f32 = tca + thc;
    var t: f32 = -1.0;
    if (t0 > 0.0) { t = t0; } else if (t1 > 0.0) { t = t1; } else {
        let bg: vec3f = vec3f(u.r, u.g, u.b);
        return vec4f(bg, u.a);
    }

    let hit: vec3f = ray_at(r, t);

    // rotate about tilted axis so north faces camera:
    let ob = 0.4091;
    let az = PI * 0.5;
    let axis = normalize(vec3f(sin(ob) * cos(az), cos(ob), sin(ob) * sin(az)));

    let speed = 0.5;
    let spin_angle = speed * u.t;

    let v = hit - C;
    let cosA = cos(spin_angle);
    let sinA = sin(spin_angle);
    let v_rot = v * cosA + cross(axis, v) * sinA + axis * dot(axis, v) * (1.0 - cosA);
    let p = C + v_rot;

    // equirect UV sampling (left unchanged)
    let cos_arg = clamp(p.y / RADIUS, -1.0, 1.0);
    let phi: f32 = atan2(p.z, p.x);
    let theta: f32 = acos(cos_arg);
    var u_coord: f32 = (phi + PI) / (2.0 * PI);
    var v_coord: f32 = theta / PI;

    u_coord = fract(u_coord);
    v_coord = clamp(v_coord, 0.0, 1.0);

    let tuv: vec2f = vec2f(u_coord, v_coord);

    var tex_col: vec3f = textureSample(tex, tex_sampler, tuv).rgb;
    let contrast = max(1.1, 0.0);
    let brightness = 0.1;
    tex_col = (tex_col - vec3f(0.5)) * contrast + vec3f(0.5) + vec3f(brightness);
    tex_col = clamp(tex_col, vec3f(0.0), vec3f(1.0));

    return vec4f(tex_col, u.a);
}


// -------------- UTILITIES -------------

// helper: shortest angular distance in [-PI, PI]
fn ang_diff(a: f32, b: f32) -> f32 {
	var d = a - b;
	// wrap to [-PI, PI]
	if (d > PI) { d = d - 2.0 * PI; }
	if (d < -PI) { d = d + 2.0 * PI; }
	return d;
}
	
// triangular function centered at 0 with halfwidth hw -> 1 at center, 0 at |d|>=hw
fn tri_from_ang_dist(d: f32, hw: f32) -> f32 {
return clamp(1.0 - abs(d) / hw, 0.0, 1.0);
}


fn rotation_matrix(x: f32, y: f32, z: f32) -> mat3x3<f32> {
    let pi: f32 = 3.141592653589793;
    let rx = x * 2.0 * pi;
    let ry = y * 2.0 * pi;
    let rz = z * 2.0 * pi;
    
    let cx = cos(rx);
    let sx = sin(rx);
    let cy = cos(ry);
    let sy = sin(ry);
    let cz = cos(rz);
    let sz = sin(rz);
    
    return mat3x3<f32>(
        vec3<f32>(cy * cz, cz * sx * sy - cx * sz, cx * cz * sy + sx * sz),
        vec3<f32>(cy * sz, cx * cz + sx * sy * sz, -cz * sx + cx * sy * sz),
        vec3<f32>(-sy, cy * sx, cx * cy)
    );
}