#version 460

layout (location = 0) in vec4 inColor;
layout (location = 1) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;

void main()
{
    outFragColor = vec4(1.0f,1.0f,1.0f,1.0f);
}