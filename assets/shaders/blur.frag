#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"

//shader input
layout (location = 0) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;

layout( push_constant ) uniform constants
{
	ivec4 handles;
} pushConstants;

void main()
{
	float weight[5] = float[] (0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
	int image = pushConstants.handles.g;
	vec2 tex_offset = 1.0 / vec2(BindlessTextureSize(image).x, -BindlessTextureSize(image).y); // gets size of single texel
	vec3 result = SampleBindlessTexture(0, image, inTexCoords).rgb * weight[0];
	bool horizontal = (pushConstants.handles.r != 0);
	if(horizontal)
	{
		for(int i = 1; i < 5; ++i)
		{
			result += SampleBindlessTexture(0, image, inTexCoords + vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
			result += SampleBindlessTexture(0, image, inTexCoords - vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
		}
	}
	else
	{
		for(int i = 1; i < 5; ++i)
		{
			result += SampleBindlessTexture(0, image, inTexCoords + vec2(tex_offset.y * i, 0.0)).rgb * weight[i];
			result += SampleBindlessTexture(0, image, inTexCoords - vec2(tex_offset.y * i, 0.0)).rgb * weight[i];
		}
	}
	outFragColor = vec4(result, 1.0);
}