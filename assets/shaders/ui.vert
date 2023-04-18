//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) out vec4 outColor;
layout (location = 1) out vec2 outTexCoords;

struct UIVertex {
	vec2 position;
	vec2 uv;
	vec4 colour;
};

layout(std140,set = 1, binding = 0) readonly buffer VertexBuffer{
	UIVertex verts[];
} uiVertices;

void main()
{
	UIVertex vertex = uiVertices.verts[gl_VertexIndex];
	outTexCoords = vertex.uv;
	outColor = vertex.colour;
	gl_Position = vec4(vertex.position.x,vertex.position.y,1.0f, 1.0f);
}