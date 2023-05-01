//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) out vec4 outColor;
layout (location = 1) out vec2 outTexCoords;
layout (location = 2) out flat int outTexHandle;

struct UIVertex {
	vec2 position;
	vec2 uv;
	vec4 colour;
	ivec4 textureHandle;
};

layout(std140,set = 1, binding = 0) uniform UIBuffer{
	vec2 screenSize;
} uiData;

layout(std140,set = 1, binding = 1) readonly buffer VertexBuffer{
	UIVertex verts[];
} uiVertices;

vec3 srgb_to_linear(vec3 srgb) {
	bvec3 cutoff = lessThan(srgb, vec3(0.04045));
	vec3 lower = srgb / vec3(12.92);
	vec3 higher = pow((srgb + vec3(0.055)) / vec3(1.055), vec3(2.4));
	return mix(higher, lower, cutoff);
}

void main()
{
	UIVertex vertex = uiVertices.verts[gl_VertexIndex];
	outTexHandle = vertex.textureHandle.r;
	outTexCoords = vertex.uv;
	outColor = vec4(srgb_to_linear(vertex.colour.rgb), vertex.colour.a);
	gl_Position = vec4(2.0 * vertex.position.x / uiData.screenSize.x - 1.0,1.0 - 2.0 * vertex.position.y / uiData.screenSize.y ,1.0f, 1.0f);
}