(goal
  (not (bvule b1
              (bvadd #x00000258
                     (bvmul #xffffffff b1)
                     (bvmul #xffffffff b2)
                     (bvmul #xffffffff b3)
                     (bvmul #xffffffff b4)
                     (bvmul #xffffffff b5))))
  (not (bvule b2 b1))
  (not (bvule b3 b2))
  (not (bvule b4 b3))
  (not (bvule b5 b4)))