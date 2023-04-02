#version 460
#extension GL_EXT_nonuniform_qualifier: enable
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"

//shader input
layout (location = 0) in vec3 inColor;
layout (location = 1) in vec2 inTexCoords;
layout (location = 2) in vec3 inNormal;

layout (location = 0) out vec4 outFragColor;

layout( push_constant ) uniform constants
{
	mat4 model;
	ivec4 textures;
} pushConstants;

void main()
{
	vec3 outColour = inColor;
	vec4 diffuseTexture = SampleBindlessTexture(pushConstants.textures.r, inTexCoords);
	if (diffuseTexture.a == 0){
		discard;
	}

	outColour *= diffuseTexture.rgb;
	outFragColor = vec4(outColour,1.0f);
}