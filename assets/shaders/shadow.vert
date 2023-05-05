//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable
#include "assets/shaders/library/camera.glsl"
#include "assets/shaders/library/object.glsl"

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec2 vTexCoords;
layout (location = 2) in vec3 vNormal;
layout (location = 3) in vec3 vColor;
layout (location = 4) in vec4 vTangent;

layout( push_constant ) uniform constants
{
	ivec4 handles;
} pushConstants;

void main()
{
	mat4 modelMatrix = modelData.models[pushConstants.handles.x].model;
	gl_Position = cameraData.sunProj * cameraData.sunView * modelMatrix * vec4(vPosition, 1.0f);
}