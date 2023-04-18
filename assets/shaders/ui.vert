//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

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
	gl_Position = vec4(1.0f,1.0f,1.0f, 1.0f);
}