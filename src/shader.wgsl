

// ------------- UNIFORMS -------------
@group(0) @binding(0) var<uniform> u: Uniforms;
struct Uniforms {
	res: vec2f,
	top_left: vec2f,
	t: f32,
	r: f32,
	g: f32,
	b: f32,
	a: f32,
	buff: f32,
}

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
// raymarch
const MAX_ITER: i32 = 32;
const GLOW_ITER: i32 = MAX_ITER/2;
const GLOW_COL:vec3f = vec3f(0.);
// wobble
const RADIUS:f32 = 1.6;
const AMP:f32 = 0.6;
const FREQ:vec3f = 4.*vec3f(0.9,1.0,1.1);
// light
const LIGHT_POS:vec3f = vec3f(0.5,0.5,2.);
const AMBIENT:f32 = 0.5;

// OTHER CONSTANTS
const CAMERA_O:vec3f = vec3f(0.,0.,10.);
const EPS: f32 = 1e-4;
const INF: f32 = 1e20;

struct Ray {
	o: vec3f, 
	d: vec3f,
} 

fn ray_at(r:Ray, d:f32)->vec3f{ return r.o + d*r.d;}
fn abort(p:vec3f)->bool{ 
	return p.z < -2.; 
}

fn sdf_grad(p:vec3f)->vec3f{
	let eps_zero:vec2f = vec2f(EPS,0.);
	return normalize(vec3f(
		sdf(p+eps_zero.xyy) - sdf(p-eps_zero.xyy),
		sdf(p+eps_zero.yxy) - sdf(p-eps_zero.yxy),
		sdf(p+eps_zero.yyx) - sdf(p-eps_zero.yyx),
	));
}

fn sdf(p:vec3f) -> f32 {
	// shift position with time
	// let l = LIGHT_POS - p;
	// let ps = p + sin(u.t)*l+l;
	let ps = p + vec3f(u.t);
	// wiggle jiggle 
	let delta:vec3f = AMP * sin(ps*FREQ);
	let dp = delta.x*delta.y*delta.z;
	// add spice to sphere
	return length(p)-RADIUS + dp;
}

fn shade(p:vec3f)->vec4f{
	let n = sdf_grad(p);
	let height_colour = colourmap((length(p)-RADIUS)/(AMP));
	let l = normalize(LIGHT_POS - p);
	// let v = normalize(CAMERA_O - p);
	return vec4f(height_colour * saturate(dot(l, n)+AMBIENT), 1.);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
	let uv = vec2(1.,-1.)*(2.*(in.position.xy-u.top_left)/u.res - vec2f(1.));
	let bg:vec3f = vec3f(u.r, u.g, u.b);

	let z_plane:f32 = 5.;
	let r:Ray = Ray(CAMERA_O, normalize(vec3f(uv, z_plane)-CAMERA_O));

	// start ray marching loop
	var d:f32 = 0.; // distance from ray origin
	var i = 0; // iteration count
	var p = r.o; // current position along the ray
	for (; i < MAX_ITER; i++){
		p = ray_at(r, d);
		if abort(p) {break;}
		let safe = sdf(p);
		if abs(safe) < EPS {
			// surface was hit!
			return shade(p);
		}
		d += safe;
	}
	// near hit
	if i >= GLOW_ITER { 
		// if iterations ran out, assume surface was almost hit
		if i == MAX_ITER { return shade(p); }; 
		// otherwise glow
		let glow:f32 = f32(i-GLOW_ITER)/f32(GLOW_ITER);
		return vec4f(mix(bg, GLOW_COL, glow*glow), u.a); 
	};
	// no hit
	return vec4f(bg, u.a);
}




// COLOUR MAP
// https://github.com/kbinani/colormap-shaders/tree/master
fn colourmap_red(x:f32) -> f32 {
	if (x < 0.09752005946586478) {
		return 5.63203907203907E+02 * x + 1.57952380952381E+02;
	} else if (x < 0.2005235116443438) {
		return 3.02650769230760E+02 * x + 1.83361538461540E+02;
	} else if (x < 0.2974133397506856) {
		return 9.21045429665647E+01 * x + 2.25581007115501E+02;
	} else if (x < 0.5003919130598823) {
		return 9.84288115246108E+00 * x + 2.50046722689075E+02;
	} else if (x < 0.5989021956920624) {
		return -2.48619704433547E+02 * x + 3.79379310344861E+02;
	} else if (x < 0.902860552072525) {
		return ((2.76764884219295E+03 * x - 6.08393126459837E+03) * x + 3.80008072407485E+03) * x - 4.57725185424742E+02;
	} else {
		return 4.27603478260530E+02 * x - 3.35293188405479E+02;
	}
}

fn colourmap_green(x:f32) -> f32 {
	if (x < 0.09785836420571035) {
		return 6.23754529914529E+02 * x + 7.26495726495790E-01;
	} else if (x < 0.2034012006283468) {
		return 4.60453201970444E+02 * x + 1.67068965517242E+01;
	} else if (x < 0.302409765476316) {
		return 6.61789401709441E+02 * x - 2.42451282051364E+01;
	} else if (x < 0.4005965758690823) {
		return 4.82379130434784E+02 * x + 3.00102898550747E+01;
	} else if (x < 0.4981907026473237) {
		return 3.24710622710631E+02 * x + 9.31717541717582E+01;
	} else if (x < 0.6064345916502067) {
		return -9.64699507389807E+01 * x + 3.03000000000023E+02;
	} else if (x < 0.7987472620841592) {
		return -2.54022986425337E+02 * x + 3.98545610859729E+02;
	} else {
		return -5.71281628959223E+02 * x + 6.51955082956207E+02;
	}
}

fn colourmap_blue(x:f32) -> f32 {
	if (x < 0.0997359608740309) {
		return 1.26522393162393E+02 * x + 6.65042735042735E+01;
	} else if (x < 0.1983790695667267) {
		return -1.22037851037851E+02 * x + 9.12946682946686E+01;
	} else if (x < 0.4997643530368805) {
		return (5.39336225400169E+02 * x + 3.55461986381562E+01) * x + 3.88081126069087E+01;
	} else if (x < 0.6025972254407099) {
		return -3.79294261294313E+02 * x + 3.80837606837633E+02;
	} else if (x < 0.6990141388105746) {
		return 1.15990231990252E+02 * x + 8.23805453805459E+01;
	} else if (x < 0.8032653181119567) {
		return 1.68464957265204E+01 * x + 1.51683418803401E+02;
	} else if (x < 0.9035796343050095) {
		return 2.40199023199020E+02 * x - 2.77279202279061E+01;
	} else {
		return -2.78813846153774E+02 * x + 4.41241538461485E+02;
	}
}

fn colourmap(x:f32)->vec3f {
	let r:f32 = saturate(colourmap_red(x) / 255.0);
	let g:f32 = saturate(colourmap_green(x) / 255.0);
	let b:f32 = saturate(colourmap_blue(x) / 255.0);
	return vec3f(r, g, b);
}