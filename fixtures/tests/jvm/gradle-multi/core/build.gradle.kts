plugins {
    id("java-library")
}

group = "com.example"
version = "1.0.0"

dependencies {
    implementation("com.google.guava:guava:33.0.0-jre")
    testImplementation("junit:junit:4.13.2")
}
