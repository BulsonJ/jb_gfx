#version 460

layout (location = 0) in vec2 outTexCoords;

layout (location = 0) out vec4 outFragColor;

layout( push_constant ) uniform constants
{
    int horizontal;
} pushConstants;


void main()
{
    outFragColor = vec4(1.0f,1.0f,1.0f,1.0f);
}