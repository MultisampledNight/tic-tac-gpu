struct Vertex {
	[[location(0)]] position: vec2<f32>;
	[[location(1)]] color: vec4<f32>;
};

struct Instance {
	[[location(2)]] offset: vec2<f32>;
};

struct ModifiedVertex {
	[[builtin(position)]] position: vec4<f32>;
	[[location(0)]] color: vec4<f32>;
};

[[stage(vertex)]]
fn vertex_main(
	source: Vertex,
	instance: Instance,
) -> ModifiedVertex {
	var out: ModifiedVertex;
	out.position = vec4<f32>(source.position + instance.offset, 0.0, 1.0);
	out.color = source.color;
	return out;
}


[[stage(fragment)]]
fn fragment_main(
	source: ModifiedVertex,
) -> [[location(0)]] vec4<f32> {
	return source.color;
}
