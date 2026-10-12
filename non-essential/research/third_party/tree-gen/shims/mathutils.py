"""Small pure-Python stand-in for the parts of Blender's mathutils that tree-gen uses.

Vector operations that Blender does in place (+=, -=, *=, /=, normalize, rotate) are in place here too,
because the generator relies on that aliasing. Results keep the class of the left operand, so subclasses
such as treegen.chturtle.Vector keep their extra methods.
"""
import math


class Vector:
    __slots__ = ("x", "y", "z")

    def __init__(self, seq=(0.0, 0.0, 0.0)):
        self.x, self.y, self.z = float(seq[0]), float(seq[1]), float(seq[2])

    # --- container behaviour
    def __getitem__(self, i):
        return (self.x, self.y, self.z)[i]

    def __setitem__(self, i, v):
        if i == 0:
            self.x = float(v)
        elif i == 1:
            self.y = float(v)
        else:
            self.z = float(v)

    def __iter__(self):
        return iter((self.x, self.y, self.z))

    def __len__(self):
        return 3

    def __repr__(self):
        return "Vector((%.4f, %.4f, %.4f))" % (self.x, self.y, self.z)

    def copy(self):
        return type(self)((self.x, self.y, self.z))

    # --- arithmetic
    def __add__(self, o):
        return type(self)((self.x + o[0], self.y + o[1], self.z + o[2]))

    __radd__ = __add__

    def __sub__(self, o):
        return type(self)((self.x - o[0], self.y - o[1], self.z - o[2]))

    def __rsub__(self, o):
        return type(self)((o[0] - self.x, o[1] - self.y, o[2] - self.z))

    def __mul__(self, s):
        return type(self)((self.x * s, self.y * s, self.z * s))

    __rmul__ = __mul__

    def __truediv__(self, s):
        return type(self)((self.x / s, self.y / s, self.z / s))

    def __neg__(self):
        return type(self)((-self.x, -self.y, -self.z))

    def __iadd__(self, o):
        self.x += o[0]
        self.y += o[1]
        self.z += o[2]
        return self

    def __isub__(self, o):
        self.x -= o[0]
        self.y -= o[1]
        self.z -= o[2]
        return self

    def __imul__(self, s):
        self.x *= s
        self.y *= s
        self.z *= s
        return self

    def __itruediv__(self, s):
        self.x /= s
        self.y /= s
        self.z /= s
        return self

    # --- geometry
    @property
    def magnitude(self):
        return math.sqrt(self.x * self.x + self.y * self.y + self.z * self.z)

    def normalize(self):
        m = self.magnitude
        if m > 0:
            self.x /= m
            self.y /= m
            self.z /= m

    def normalized(self):
        v = self.copy()
        v.normalize()
        return v

    def dot(self, o):
        return self.x * o[0] + self.y * o[1] + self.z * o[2]

    def cross(self, o):
        return type(self)((self.y * o[2] - self.z * o[1], self.z * o[0] - self.x * o[2], self.x * o[1] - self.y * o[0]))

    def angle(self, o, fallback=None):
        m = self.magnitude * math.sqrt(o[0] * o[0] + o[1] * o[1] + o[2] * o[2])
        if m == 0:
            return fallback
        return math.acos(max(-1.0, min(1.0, self.dot(o) / m)))

    def rotate(self, q):
        m = q.m
        x, y, z = self.x, self.y, self.z
        self.x = m[0][0] * x + m[0][1] * y + m[0][2] * z
        self.y = m[1][0] * x + m[1][1] * y + m[1][2] * z
        self.z = m[2][0] * x + m[2][1] * y + m[2][2] * z

    def to_track_quat(self, track="Y", up="Z"):
        """Rotation taking +Z to this vector, with local +Y leaning toward world +Z. Only ('Z', 'Y') is needed."""
        assert (track, up) == ("Z", "Y")
        z = self.normalized()
        upv = Vector((0.0, 0.0, 1.0)) - z * z.z
        if upv.magnitude < 1e-6:
            upv = Vector((0.0, 1.0, 0.0))
        upv.normalize()
        xv = upv.cross(z)
        m = [[xv.x, upv.x, z.x], [xv.y, upv.y, z.y], [xv.z, upv.z, z.z]]
        return Quaternion(m)


class Quaternion:
    """A rotation, stored as a 3x3 matrix. Built from an axis and angle, or directly from a matrix."""

    def __init__(self, a, angle=None):
        if angle is None:
            self.m = [list(map(float, r)) for r in a]
            return
        x, y, z = a[0], a[1], a[2]
        n = math.sqrt(x * x + y * y + z * z)
        if n == 0:
            self.m = [[1.0, 0, 0], [0, 1.0, 0], [0, 0, 1.0]]
            return
        x, y, z = x / n, y / n, z / n
        c, s = math.cos(angle), math.sin(angle)
        t = 1 - c
        self.m = [[t * x * x + c, t * x * y - s * z, t * x * z + s * y],
                  [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
                  [t * x * z - s * y, t * y * z + s * x, t * z * z + c]]

    def inverted(self):
        m = self.m
        return Quaternion([[m[j][i] for j in range(3)] for i in range(3)])
