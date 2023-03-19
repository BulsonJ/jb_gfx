#version 460
#extension GL_EXT_nonuniform_qualifier: enable

//shader input
layout (location = 0) in vec3 inColor;
layout (location = 1) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;

layout (set = 0, binding = 1) uniform sampler2D bindlessTextures[];

void main()
{
	vec3 outColour = inColor;
	outFragColor = vec4(outColour,1.0f);
}