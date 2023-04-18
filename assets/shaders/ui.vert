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

void main()
{
	UIVertex vertex = uiVertices.verts[gl_VertexIndex];
	outTexHandle = vertex.textureHandle.r;
	outTexCoords = vertex.uv;
	outColor = vertex.colour;
	gl_Position = vec4(2.0 * vertex.position.x / uiData.screenSize.x - 1.0,1.0 - 2.0 * vertex.position.y / uiData.screenSize.y ,1.0f, 1.0f);
}